use std::{mem, num::NonZeroU32};

use ahash::AHashMap;
use bytemuck::{Pod, Zeroable};
// use rayon::prelude::*;
use swash::{shape::ShapeContext, FontRef};
use termwiz::{color::ColorAttribute, surface::SequenceNo};
use wgpu::SamplerBindingType;
use wgpu_container::{WgpuCell, WgpuVec};

use super::{FontTexture, GlyphCacheInfo, TEXTURE_WIDTH};
use crate::render::Viewport;
use wezterm_term::{StableRowIndex, Terminal};

const SCROLLBAR_FOCUSED: [f32; 4] = [0.2, 0.2, 0.2, 1.0];
const SCROLLBAR_UNFOCUSED: [f32; 4] = [0.6, 0.6, 0.6, 1.0];

pub struct CellContext {
    pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    ui_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    instances: WgpuVec<CellVertex>,
    text_instances: WgpuVec<TextVertex>,
    ui: WgpuCell<Ui>,
    window_size: WgpuCell<WindowSize>,
    font: FontRef<'static>,
    font_size: f32,
    font_descent: f32,
    glyph_cache: AHashMap<u16, GlyphCacheInfo>,
    prev_term_seqno: SequenceNo,
    scroll_offset: StableRowIndex,
    mouse_status: MouseStatus,
    shape_ctx: ShapeContext,
}

impl CellContext {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: &Viewport,
        font_texture: FontTexture,
        font_size: f32,
        scale_factor: f32,
    ) -> Self {
        profiling::scope!("Create CellContext");

        let font_size = font_size * scale_factor;

        let font = font_texture.font;

        let metrics = font.metrics(&[]).scale(font_size);
        // monospace width
        assert!(metrics.is_monospace);
        let glyph_metrics = font.glyph_metrics(&[]).scale(font_size);
        let font_width = glyph_metrics.advance_width(font.charmap().map('M'));
        let font_height = metrics.ascent + metrics.descent;
        let cell_size = [font_width, font_height];

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("size_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(&wgpu::include_wgsl!("../shaders/shader.wgsl"));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cell_pipeline"),
            multiview: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "cell_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CellVertex>() as _,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4,
                        1 => Float32x2,
                        2 => Float32x2,
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "cell_fs",
                targets: &[wgpu::ColorTargetState {
                    format: viewport.format(),
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
        });

        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui_pipeline"),
            multiview: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "ui_vs",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "cell_fs",
                targets: &[wgpu::ColorTargetState {
                    format: viewport.format(),
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
        });

        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text_pipeline"),
            multiview: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "text_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TextVertex>() as _,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x2,
                        3 => Float32x3,
                        4 => Sint32,
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "text_fs",
                targets: &[wgpu::ColorTargetState {
                    format: viewport.format(),
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
        });

        let window_size = WgpuCell::new(
            device,
            wgpu::BufferUsages::UNIFORM,
            WindowSize {
                size: [viewport.width() as f32, viewport.height() as f32],
                cell_size,
                column: crate::COLUMN,
                pad: 0,
            },
        );
        let ui = WgpuCell::new(
            device,
            wgpu::BufferUsages::UNIFORM,
            Ui {
                cursor_color: [1.0; 4],
                cursor_pos: [0.0; 2],
                scrollbar_width: 15.0 * scale_factor,
                scrollbar_height: 2.0,
                scrollbar_bg: [1.0; 4],
                scrollbar_fg: SCROLLBAR_UNFOCUSED,
                scrollbar_top: -1.0,
                pad: [0.0; 3],
            },
        );

        let texture_size = wgpu::Extent3d {
            width: TEXTURE_WIDTH,
            height: TEXTURE_WIDTH,
            depth_or_array_layers: font_texture.layer_count,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Font texture"),
            format: wgpu::TextureFormat::R8Unorm,
            dimension: wgpu::TextureDimension::D2,
            sample_count: 1,
            mip_level_count: 1,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            size: texture_size,
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        queue.write_texture(
            texture.as_image_copy(),
            &font_texture.data,
            wgpu::ImageDataLayout {
                bytes_per_row: NonZeroU32::new(TEXTURE_WIDTH),
                rows_per_image: NonZeroU32::new(TEXTURE_WIDTH),
                offset: 0,
            },
            texture_size,
        );

        let font_texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("window size bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: window_size.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: ui.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&font_texture_sampler),
                },
            ],
        });

        Self {
            scroll_offset: 0,
            prev_term_seqno: 0,
            text_instances: WgpuVec::new(device, wgpu::BufferUsages::VERTEX),
            instances: WgpuVec::new(device, wgpu::BufferUsages::VERTEX),
            bind_group,
            glyph_cache: font_texture.glyph_cache,
            shape_ctx: ShapeContext::new(),
            window_size,
            ui,
            font,
            font_size,
            font_descent: metrics.descent,
            pipeline,
            text_pipeline,
            ui_pipeline,
            mouse_status: MouseStatus::default(),
        }
    }

    #[profiling::function]
    pub fn click(&mut self, _x: f32, _y: f32) -> bool {
        false
    }

    #[profiling::function]
    pub fn hover(&mut self, x: f32, y: f32) -> bool {
        let target = self.ui.target(self.window_size.size, x, y);

        match self.mouse_status {
            MouseStatus::Hover(ref mut old_target) => {
                if *old_target == target {
                    false
                } else {
                    match target {
                        MouseTarget::Empty => {
                            self.ui.update(|ui| {
                                ui.scrollbar_fg = SCROLLBAR_UNFOCUSED;
                            });
                        }
                        MouseTarget::ScrollBar => {
                            self.ui.update(|ui| {
                                ui.scrollbar_fg = SCROLLBAR_FOCUSED;
                            });
                        }
                    }

                    *old_target = target;

                    true
                }
            }
            MouseStatus::Drag { .. } => unreachable!(),
        }
    }

    #[profiling::function]
    pub fn drag_end(&mut self) {
        match mem::take(&mut self.mouse_status) {
            MouseStatus::Hover(_) => unreachable!(),
            MouseStatus::Drag { target, .. } => match target {
                MouseTarget::Empty => {}
                MouseTarget::ScrollBar => {
                    self.ui.update(|ui| {
                        ui.scrollbar_fg = SCROLLBAR_UNFOCUSED;
                    });
                }
            },
        }
    }

    #[profiling::function]
    pub fn drag(&mut self, x: f32, y: f32) -> bool {
        let target = self.ui.target(self.window_size.size, x, y);

        match self.mouse_status {
            MouseStatus::Hover(_) => {
                match target {
                    MouseTarget::ScrollBar => {
                        self.ui.update(|ui| {
                            ui.scrollbar_fg = SCROLLBAR_FOCUSED;
                        });
                    }
                    MouseTarget::Empty => {
                        self.ui.update(|ui| {
                            ui.scrollbar_fg = SCROLLBAR_UNFOCUSED;
                        });
                    }
                }
                self.mouse_status = MouseStatus::Drag {
                    target,
                    current: (x, y),
                    start: (x, y),
                };
                true
            }
            MouseStatus::Drag {
                ref mut current, ..
            } => {
                let new_current = (x, y);
                if *current != new_current {
                    *current = new_current;
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.window_size.update(|size| {
            size.size = [width, height];
        });
    }

    #[profiling::function]
    pub fn scroll(&mut self, offset: StableRowIndex, term: &Terminal) {
        let screen = term.screen();
        let min = 0;
        let max = screen.visible_row_to_stable_row(0);
        self.scroll_offset = (self.scroll_offset + offset).max(min).min(max);
    }

    pub fn scroll_to_bottom(&mut self, term: &Terminal) {
        self.scroll_offset = term.screen().visible_row_to_stable_row(0);
    }

    #[profiling::function]
    pub fn set_terminal(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, term: &Terminal) {
        let screen = term.screen();
        let palette = term.get_config().color_palette();

        // self.desired_size = [
        //     screen.physical_cols as f32 * self.window_size.cell_size[0] + self.ui.scrollbar_width,
        //     screen.physical_rows as f32 * self.window_size.cell_size[1],
        // ];
        let cell_size = self.window_size.cell_size;

        {
            profiling::scope!("Make cell instances");
            let lines = screen.lines.as_slices().0;
            let cells = (0..screen.physical_cols)
                .into_iter()
                .zip(0..screen.physical_rows)
                .filter_map(|(x, y)| {
                    let cell = lines[y].cells().get(x)?;

                    if cell.attrs().background() != ColorAttribute::Default {
                        let cell_pos = [x as f32 * cell_size[0], y as f32 * cell_size[1]];
                        let color = palette.resolve_bg(cell.attrs().background());
                        let (r, g, b, _) = color.to_tuple_rgba();
                        Some(CellVertex {
                            color: [r, g, b, 1.0],
                            cell_pos,
                            pad: [0.0; 2],
                        })
                    } else {
                        None
                    }
                });
            self.instances.cpu_buffer_mut().clear();
            self.instances.cpu_buffer_mut().extend(cells);
        }

        {
            profiling::scope!("Make text instances");

            self.text_instances.cpu_buffer_mut().clear();

            let start = self.scroll_offset;
            let end = self.scroll_offset + screen.physical_rows as StableRowIndex;
            let range = screen.stable_range(&(start..end));

            self.ui.update(|ui| {
                ui.cursor_pos = [
                    term.cursor_pos().x as _,
                    screen.phys_row(term.cursor_pos().y) as _,
                ];
                let full_height = screen.lines.as_slices().0.len() as f32;

                ui.scrollbar_top = 1.0 - (range.start as f32 / full_height) * 2.0;
                ui.scrollbar_height = -(range.len() as f32 / full_height) * 2.0;
            });

            for (line_no, line) in screen.lines.as_slices().0[range].iter().enumerate() {
                // if !line.changed_since(self.prev_term_seqno) {
                //     continue;
                // }
                let mut x = 0.0;
                let mut shaper = self
                    .shape_ctx
                    .builder(self.font)
                    .size(self.font_size)
                    .build();
                let s = line.as_str();
                shaper.add_str(&s);
                let mut cells = line.cells();

                shaper.shape_with(|cluster| {
                    let (cluster_cells, new_cells) = cells.split_at(cluster.glyphs.len());
                    cells = new_cells;
                    // let s = &s[cluster.source.to_range()];
                    for (glyph, cell) in cluster.glyphs.iter().zip(cluster_cells) {
                        if let Some(info) = self.glyph_cache.get(&glyph.id) {
                            let (r, g, b, _) = palette
                                .resolve_fg(cell.attrs().foreground())
                                .to_tuple_rgba();
                            self.text_instances.cpu_buffer_mut().push(TextVertex {
                                offset: [
                                    x + glyph.x + info.glyph_position[0],
                                    cell_size[1] * (line_no + 1) as f32
                                        - (info.glyph_position[1] + glyph.y + self.font_descent),
                                ],
                                tex_offset: info.tex_position,
                                tex_size: info.tex_size,
                                color: [r, g, b],
                                layer: info.layer as i32,
                            });
                        }
                        x += glyph.advance;
                    }
                });
            }
        }

        self.instances.write(device, queue);
        self.text_instances.write(device, queue);
        self.prev_term_seqno = term.current_seqno();
    }

    #[profiling::function]
    pub fn draw<'a>(&'a mut self, queue: &wgpu::Queue, rpass: &mut wgpu::RenderPass<'a>) {
        self.window_size.flush(queue);
        self.ui.flush(queue);

        rpass.set_bind_group(0, &self.bind_group, &[]);

        if self.instances.len() != 0 {
            rpass.push_debug_group("Draw cell");
            rpass.set_pipeline(&self.pipeline);
            rpass.set_vertex_buffer(0, self.instances.gpu_buffer().slice(..));
            rpass.draw(0..4, 0..self.instances.len() as _);
            rpass.pop_debug_group();
        }

        rpass.push_debug_group("Draw text");
        rpass.set_pipeline(&self.text_pipeline);
        rpass.set_vertex_buffer(0, self.text_instances.gpu_buffer().slice(..));
        rpass.draw(0..4, 0..self.text_instances.len() as _);
        rpass.pop_debug_group();

        rpass.push_debug_group("Draw ui");
        rpass.set_pipeline(&self.ui_pipeline);
        // cursor, scrollbar outer, scrollbar inner
        rpass.draw(0..4, 0..3);
        rpass.pop_debug_group();
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct CellVertex {
    color: [f32; 4],
    cell_pos: [f32; 2],
    pad: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct TextVertex {
    offset: [f32; 2],
    tex_offset: [f32; 2],
    tex_size: [f32; 2],
    color: [f32; 3],
    layer: i32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct WindowSize {
    size: [f32; 2],
    cell_size: [f32; 2],
    column: u32,
    pad: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Ui {
    cursor_color: [f32; 4],
    cursor_pos: [f32; 2],
    scrollbar_width: f32,
    scrollbar_height: f32,
    scrollbar_fg: [f32; 4],
    scrollbar_bg: [f32; 4],
    scrollbar_top: f32,
    pad: [f32; 3],
}

impl Ui {
    pub fn target(&self, [width, height]: [f32; 2], x: f32, y: f32) -> MouseTarget {
        let scrollbar_left = width - self.scrollbar_width;
        let y_ndc = 1.0 - (y * 2.0 / height);

        let cursor_in_scrollbar = x >= scrollbar_left
            && y_ndc <= self.scrollbar_top
            && y_ndc >= (self.scrollbar_top + self.scrollbar_height);

        if cursor_in_scrollbar {
            MouseTarget::ScrollBar
        } else {
            MouseTarget::Empty
        }
    }
}

static_assertions::assert_eq_size!(Ui, [f32; 20]);
static_assertions::assert_eq_size!(WindowSize, [u8; 24]);

#[derive(Clone, Copy, PartialEq, Eq)]
enum MouseTarget {
    Empty,
    ScrollBar,
}

#[derive(Clone, Copy, PartialEq)]
enum MouseStatus {
    Hover(MouseTarget),
    Drag {
        target: MouseTarget,
        start: (f32, f32),
        current: (f32, f32),
    },
}

impl Default for MouseStatus {
    fn default() -> Self {
        Self::Hover(MouseTarget::Empty)
    }
}
