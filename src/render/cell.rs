use std::num::NonZeroU32;

use bytemuck::{Pod, Zeroable};
use fontdue::{
    layout::{CoordinateSystem, Layout, TextStyle},
    Font, FontSettings,
};
use rayon::prelude::*;
use wgpu::util::DeviceExt;

use crate::term::Terminal;

use super::Viewport;

const TEXTURE_WIDTH: u32 = 2048;
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
    font: Font,
    font_size: f32,
}

impl CellContext {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: &Viewport,
        font_size: f32,
    ) -> Self {
        let font = Font::from_bytes(
            super::FONT,
            FontSettings {
                collection_index: 0,
                scale: font_size,
            },
        )
        .unwrap();

        let font_width = font.metrics('M', font_size).advance_width.ceil();
        let cell_size = [font_width, font_size];

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
                        2 => Float32x3,
                        3 => Uint32,
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

        let instance_count = 20;

        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cell Vertex Buffer"),
            contents: bytemuck::cast_slice(&create_cell_instance(instance_count)),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let cell_width = cell_size[0] as u32;
        let cell_height = cell_size[1] as u32;
        let text_per_column = TEXTURE_WIDTH / cell_width;
        let text_per_row = TEXTURE_WIDTH / cell_height;
        let layer_count = (font.glyph_count() as u32 / (text_per_row * text_per_column)).max(2);

        let window_size = WindowSize {
            size: [viewport.width() as f32, viewport.height() as f32],
            texture_count: [text_per_column, text_per_row],
            cell_size,
            column: 5,
        };
        let window_size_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&[window_size]),
            label: Some("window size buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let texture_size = wgpu::Extent3d {
            width: TEXTURE_WIDTH,
            height: TEXTURE_WIDTH,
            depth_or_array_layers: layer_count,
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

        let mut data = vec![0u8; TEXTURE_SIZE * layer_count as usize];

        data.par_chunks_exact_mut(TEXTURE_SIZE)
            .enumerate()
            .for_each(|(layer, page)| {
                let glyph_id_base = (layer * (text_per_row * text_per_column) as usize) as u16;
                for row_index in 0..text_per_row as usize {
                    let index_base_row = row_index * cell_height as usize * TEXTURE_WIDTH as usize;
                    let glyph_id_row = glyph_id_base + text_per_column as u16 * row_index as u16;
                    for column_index in 0..text_per_column as usize {
                        let glyph_id = glyph_id_row + column_index as u16;
                        if font.glyph_count() <= glyph_id {
                            return;
                        }
                        let (metric, raster) = font.rasterize_indexed(glyph_id, font_size);
                        if raster.is_empty() {
                            continue;
                        }

                        let index_base = index_base_row + column_index * cell_width as usize;

                        for (row, raster_row) in raster.chunks_exact(metric.width).enumerate() {
                            let start = index_base + row * TEXTURE_WIDTH as usize;
                            let end = start + raster_row.len();
                            page[start..end].copy_from_slice(raster_row);
                        }
                    }
                }
            });

        // use std::io::Write;
        // let mut out = std::fs::OpenOptions::new()
        //     .write(true)
        //     .create(true)
        //     .open("foo.pgm")
        //     .unwrap();
        // write!(out, "P5\n{} {}\n255\n", TEXTURE_WIDTH, TEXTURE_WIDTH).unwrap();
        // out.write_all(&data[..TEXTURE_SIZE]).unwrap();
        // out.flush().unwrap();

        queue.write_texture(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                texture: &texture,
            },
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
            text_instances,
            text_instance_count: 0,
            instances,
            instance_count,
            bind_group,
            window_size_buf,
            font,
            font_size,
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
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        let mut t = String::new();

        for line in term.rows() {
            line.write_text(&mut t);
            t.push('\n');

            layout.append(&[&self.font], &TextStyle::new(&t, self.font_size, 0));

            t.clear();
        }

        let vertexes = layout
            .glyphs()
            .par_iter()
            .map(|g| TextVertex {
                offset: [g.x, g.y],
                tex_size: [g.width as f32, g.height as f32],
                color: [1.0, 1.0, 1.0],
                glyph_id: g.key.glyph_index as u32,
            })
            .collect::<Vec<_>>();
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
    tex_size: [f32; 2],
    color: [f32; 3],
    glyph_id: u32,
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
    texture_count: [u32; 2],
    column: u32,
}
