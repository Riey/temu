use bytemuck::{Pod, Zeroable};
use lyon::{geom::Point, lyon_tessellation::FillVertexConstructor, path::path::Builder};

use super::Viewport;

pub struct LyonContext {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl LyonContext {
    pub fn new(device: &wgpu::Device, viewport: &Viewport) -> Self {
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("lyon bind_group_layout"),
                entries: &[],
            });

        let shader = device.create_shader_module(&wgpu::include_wgsl!("../shaders/lyon.wgsl"));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[],
            label: Some("lyon bind_group"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lyon_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "lyon_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LyonVertex>() as _,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "lyon_fs",
                targets: &[viewport.format().into()],
            }),
            primitive: wgpu::PrimitiveState {
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                ..Default::default()
            },
        });

        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Lyon vertex buffer"),
            size: 0,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Lyon vertex buffer"),
            size: 0,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });


        Self {
            bind_group,
            index_buf,
            vertex_buf,
            pipeline,
        }
    }
}

struct LyonBuilder {
    builder: Builder,
}

impl ttf_parser::OutlineBuilder for LyonBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.begin(Point::new(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(Point::new(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder
            .quadratic_bezier_to(Point::new(x1, y1), Point::new(x, y));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder
            .cubic_bezier_to(Point::new(x1, y1), Point::new(x2, y2), Point::new(x, y));
    }

    fn close(&mut self) {
        self.builder.close();
    }
}


#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct LyonVertex {
    position: [f32; 2],
}

struct VertexCtor {}

impl FillVertexConstructor<LyonVertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: lyon::lyon_tessellation::FillVertex) -> LyonVertex {
        LyonVertex {
            position: vertex.position().to_array(),
        }
    }
}