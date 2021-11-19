use std::num::NonZeroU32;

use ahash::AHashMap;
use bytemuck::{Pod, Zeroable};
use swash::{
    scale::{image::Image, Render, ScaleContext, Source, StrikeWith},
    shape::ShapeContext,
    FontRef,
};
use termwiz::surface::SequenceNo;
use wgpu_container::{WgpuCell, WgpuVec};

use crate::render::atals::ArrayAllocator;
use crate::render::Viewport;
use wezterm_term::Terminal;

const TEXTURE_WIDTH: u32 = 1024;
const TEXTURE_SIZE: usize = (TEXTURE_WIDTH * TEXTURE_WIDTH) as usize;

pub struct CellContext {
    pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    instances: WgpuVec<CellVertex>,
    text_instances: WgpuVec<TextVertex>,
    window_size: WgpuCell<WindowSize>,
    font: FontRef<'static>,
    font_size: f32,
    font_descent: f32,
    desired_size: [f32; 2],
    glyph_cache: AHashMap<u16, GlyphInfo>,
    shape_ctx: ShapeContext,
    prev_term_seqno: SequenceNo,
}

impl CellContext {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: &Viewport,
        font_size: f32,
    ) -> Self {
        let font_size = font_size;

        let font = FontRef::from_index(super::FONT, 0).unwrap();

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
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler {
                        comparison: false,
                        filtering: true,
                    },
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
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "cell_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CellVertex>() as _,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4,
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

        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text_pipeline"),
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
            },
        );

        let mut allocator = ArrayAllocator::new(TEXTURE_WIDTH, TEXTURE_WIDTH);

        let mut glyph_cache = AHashMap::new();
        let shape_ctx = ShapeContext::new();
        let mut scale_ctx = ScaleContext::new();
        let mut image = Image::new();
        let mut data = vec![0; TEXTURE_SIZE * 2];
        let mut layer_count = 1;

        let mut scaler = scale_ctx.builder(font).size(font_size).build();

        font.charmap().enumerate(|_c, id| {
            image.clear();
            if Render::new(&[
                Source::ColorBitmap(StrikeWith::BestFit),
                Source::ColorOutline(0),
                Source::Bitmap(StrikeWith::BestFit),
                Source::Outline,
            ])
            .render_into(&mut scaler, id, &mut image)
            {
                if image.placement.width == 0 || image.placement.height == 0 {
                } else {
                    let alloc = allocator.alloc(image.placement.width, image.placement.height);
                    if let Some(new_page) = alloc.layer.checked_sub(layer_count) {
                        data.extend(std::iter::repeat(0).take(TEXTURE_SIZE * new_page as usize));
                        layer_count += new_page;
                    }
                    let page = &mut data[TEXTURE_SIZE * alloc.layer as usize..][..TEXTURE_SIZE];
                    let left_top = (alloc.y * TEXTURE_WIDTH + alloc.x) as usize;

                    for (row_index, row) in image
                        .data
                        .chunks_exact(image.placement.width as usize)
                        .enumerate()
                    {
                        let begin = left_top + row_index * TEXTURE_WIDTH as usize;
                        let end = begin + row.len();
                        page[begin..end].copy_from_slice(row);
                    }
                    glyph_cache.insert(
                        id,
                        GlyphInfo {
                            tex_position: [alloc.x as _, alloc.y as _],
                            tex_size: [image.placement.width as _, image.placement.height as _],
                            glyph_position: [image.placement.left as _, image.placement.top as _],
                            layer: alloc.layer as _,
                        },
                    );
                }
            }
        });

        use std::io::Write;
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open("foo.pgm")
            .unwrap();
        write!(out, "P5\n{} {}\n255\n", TEXTURE_WIDTH, TEXTURE_WIDTH).unwrap();
        out.write_all(&data[..TEXTURE_SIZE]).unwrap();
        out.flush().unwrap();

        let texture_size = wgpu::Extent3d {
            width: TEXTURE_WIDTH,
            height: TEXTURE_WIDTH,
            depth_or_array_layers: allocator.layer_count(),
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
            &data,
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
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&font_texture_sampler),
                },
            ],
        });

        Self {
            shape_ctx,
            prev_term_seqno: 0,
            desired_size: [0.0, 0.0],
            text_instances: WgpuVec::new(device, wgpu::BufferUsages::VERTEX),
            instances: WgpuVec::new(device, wgpu::BufferUsages::VERTEX),
            bind_group,
            glyph_cache,
            window_size,
            font,
            font_size,
            font_descent: metrics.descent,
            pipeline,
            text_pipeline,
        }
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: f32, height: f32) {
        self.window_size.update(queue, |size| {
            size.size = [width, height];
        });
    }

    pub fn set_terminal(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, term: &Terminal) {
        let screen = term.screen();

        let mut t = String::new();

        // queue.write_buffer(
        //     &self.instances,
        //     self.prev_cursor as _,
        //     bytemuck::cast_slice(&[CellVertex { color: [0.0; 4] }]),
        // );
        // if let Ok(y) = usize::try_from(term.cursor_pos().y) {
        //     let cursor = term.cursor_pos().x + y * screen.physical_cols;
        //     dbg!(term.cursor_pos(), cursor);
        // }
        // queue.write_buffer(
        //     &self.instances,
        //     cursor as _,
        //     bytemuck::cast_slice(&[CellVertex { color: [1.0; 4] }]),
        // );

        self.desired_size = [
            screen.physical_cols as f32 * self.window_size.cell_size[0],
            screen.lines.len() as f32 * self.window_size.cell_size[1],
        ];
        self.instances.cpu_buffer_mut().resize(
            screen.physical_cols * screen.physical_rows,
            CellVertex {
                color: [0.1, 0.1, 0.1, 1.0],
            },
        );
        self.text_instances.cpu_buffer_mut().clear();

        for (line_no, line) in screen.lines.iter().enumerate() {
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
            let palette = term.get_config().color_palette();

            shaper.shape_with(|cluster| {
                let (cluster_cells, new_cells) = cells.split_at(cluster.glyphs.len());
                cells = new_cells;
                assert!(!cluster.is_ligature());
                // let s = &t[cluster.source.to_range()];
                for (glyph, cell) in cluster.glyphs.iter().zip(cluster_cells) {
                    if let Some(info) = self.glyph_cache.get(&glyph.id) {
                        let (r, g, b, _) = palette
                            .resolve_fg(cell.attrs().foreground())
                            .to_tuple_rgba();
                        self.text_instances.cpu_buffer_mut().push(TextVertex {
                            offset: [
                                x + glyph.x + info.glyph_position[0],
                                self.window_size.cell_size[1] * (line_no + 1) as f32
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

            t.clear();
        }

        self.instances.write(device, queue);
        self.text_instances.write(device, queue);
        self.prev_term_seqno = term.current_seqno();
    }

    pub fn desired_size(&self) -> [f32; 2] {
        self.desired_size
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>) {
        rpass.set_bind_group(0, &self.bind_group, &[]);

        rpass.push_debug_group("Draw cell");
        rpass.set_pipeline(&self.pipeline);
        rpass.set_vertex_buffer(0, self.instances.gpu_buffer().slice(..));
        rpass.draw(0..4, 0..self.instances.len() as _);
        rpass.pop_debug_group();

        rpass.push_debug_group("Draw text");
        rpass.set_pipeline(&self.text_pipeline);
        rpass.set_vertex_buffer(0, self.text_instances.gpu_buffer().slice(..));
        rpass.draw(0..4, 0..self.text_instances.len() as _);
        rpass.pop_debug_group();
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct CellVertex {
    color: [f32; 4],
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
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct WindowSize {
    size: [f32; 2],
    cell_size: [f32; 2],
    column: u32,
}

struct GlyphInfo {
    tex_position: [f32; 2],
    glyph_position: [f32; 2],
    tex_size: [f32; 2],
    layer: i32,
}
