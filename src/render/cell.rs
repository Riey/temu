use std::{mem::size_of, num::NonZeroU32, time::Instant};

use ahash::AHashMap;
use bytemuck::{Pod, Zeroable};
use rayon::prelude::*;
use swash::{
    scale::{image::Image, Render, ScaleContext, Source, StrikeWith},
    shape::ShapeContext,
    FontRef,
};
use wgpu::util::DeviceExt;

use crate::render::atals::ArrayAllocator;
use crate::term::Terminal;

use super::{atals::Allocation, Viewport};

const TEXTURE_WIDTH: u32 = 1024;
const TEXTURE_SIZE: usize = (TEXTURE_WIDTH * TEXTURE_WIDTH) as usize;

pub struct CellContext {
    pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    instance_count: usize,
    text_instances: wgpu::Buffer,
    text_instance_count: usize,
    window_size_buf: wgpu::Buffer,
    font: FontRef<'static>,
    font_size: f32,
    font_descent: f32,
    cell_size: [f32; 2],
    glyph_cache: AHashMap<u16, GlyphInfo>,
    shape_ctx: ShapeContext,
    prev_cursor: usize,
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

        let instance_count = (crate::COLUMN * crate::ROW) as usize;

        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cell Vertex Buffer"),
            contents: bytemuck::cast_slice(&create_cell_instance(instance_count)),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let window_size = WindowSize {
            size: [viewport.width() as f32, viewport.height() as f32],
            cell_size,
            column: crate::COLUMN,
        };
        let window_size_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&[window_size]),
            label: Some("window size buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

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
                    resource: window_size_buf.as_entire_binding(),
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

        let text_instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text instance buffer"),
            mapped_at_creation: false,
            size: (std::mem::size_of::<TextVertex>() * instance_count) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            shape_ctx,
            prev_cursor: 0,
            text_instances,
            text_instance_count: 0,
            instances,
            instance_count,
            bind_group,
            glyph_cache,
            window_size_buf,
            cell_size,
            font,
            font_size,
            font_descent: metrics.descent,
            pipeline,
            text_pipeline,
        }
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        queue.write_buffer(
            &self.window_size_buf,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32]),
        );
    }

    pub fn set_terminal(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, term: &Terminal) {
        let mut t = String::new();
        let mut vertexes = Vec::new();

        queue.write_buffer(
            &self.instances,
            self.prev_cursor as _,
            bytemuck::cast_slice(&[CellVertex { color: [0.0; 4] }]),
        );
        let cursor = term.cursor_pos() * size_of::<CellVertex>();
        self.prev_cursor = cursor;
        queue.write_buffer(
            &self.instances,
            cursor as _,
            bytemuck::cast_slice(&[CellVertex { color: [1.0; 4] }]),
        );

        for (line_no, line) in term.rows().enumerate() {
            let mut x = 0.0;
            let mut shaper = self
                .shape_ctx
                .builder(self.font)
                .size(self.font_size)
                .build();
            line.write_text(&mut t);
            shaper.add_str(&t);

            shaper.shape_with(|cluster| {
                assert!(!cluster.is_ligature());
                // let s = &t[cluster.source.to_range()];
                for glyph in cluster.glyphs {
                    if let Some(info) = self.glyph_cache.get(&glyph.id) {
                        vertexes.push(TextVertex {
                            offset: [
                                x + glyph.x + info.glyph_position[0],
                                self.cell_size[1] * (line_no + 1) as f32
                                    - (info.glyph_position[1] + glyph.y + self.font_descent),
                            ],
                            tex_offset: info.tex_position,
                            tex_size: info.tex_size,
                            color: [1.0; 3],
                            layer: info.layer as i32,
                        });
                    }
                    x += glyph.advance;
                }
            });

            t.clear();
        }

        if self.text_instance_count >= vertexes.len() {
            queue.write_buffer(&self.text_instances, 0, bytemuck::cast_slice(&vertexes));
        } else {
            self.text_instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("text instance"),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                contents: bytemuck::cast_slice(&vertexes),
            });
        }

        self.text_instance_count = vertexes.len();
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>) {
        rpass.set_bind_group(0, &self.bind_group, &[]);

        rpass.push_debug_group("Draw cell");
        rpass.set_pipeline(&self.pipeline);
        rpass.set_vertex_buffer(0, self.instances.slice(..));
        rpass.draw(0..4, 0..self.instance_count as u32);
        rpass.pop_debug_group();

        rpass.push_debug_group("Draw text");
        rpass.set_pipeline(&self.text_pipeline);
        rpass.set_vertex_buffer(0, self.text_instances.slice(..));
        rpass.draw(0..4, 0..self.text_instance_count as u32);
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

fn create_cell_instance(count: usize) -> Vec<CellVertex> {
    std::iter::repeat_with(|| CellVertex {
        color: [0.0, 0.0, 0.0, 0.0],
    })
    .take(count)
    .collect()
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WindowSize {
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
