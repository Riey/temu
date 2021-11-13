use bytemuck::{Pod, Zeroable};
use lyon::{
    geom::Point,
    lyon_tessellation::{
        BuffersBuilder, FillOptions, FillTessellator, FillVertexConstructor, VertexBuffers,
    },
    path::path::Builder,
};
use ttf_parser::Face;
use wgpu::util::DeviceExt;

use super::Viewport;

pub struct LyonContext {
    globals_buf: wgpu::Buffer,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: usize,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    face: Face<'static>,
}

impl LyonContext {
    pub fn new(device: &wgpu::Device, viewport: &Viewport) -> Self {
        let face = Face::from_slice(super::FONT, 0).unwrap();

        let globals = Globals {
            window_size: [viewport.width() as _, viewport.height() as _],
            font_size: super::FONT_SIZE as f32,
        };

        let globals_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lyon globals buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&[globals]),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lyon bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let shader = device.create_shader_module(&wgpu::include_wgsl!("../shaders/lyon.wgsl"));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(globals_buf.as_entire_buffer_binding()),
            }],
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
            face,
            bind_group,
            globals_buf,
            index_buf,
            vertex_buf,
            index_count: 0,
            pipeline,
        }
    }

    pub fn set_draw(&mut self, device: &wgpu::Device, ch: char) {
        let glyph = self.face.glyph_index(ch).unwrap();

        let mut tess = FillTessellator::new();
        let mut builder = LyonBuilder {
            builder: Builder::new(),
        };
        let output = self.face.outline_glyph(glyph, &mut builder).unwrap();
        let height = output.height() as f32;
        let mut mesh = VertexBuffers::<LyonVertex, u32>::new();
        let path = builder.builder.build();
        tess.tessellate_path(
            &path,
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut mesh, VertexCtor { height }),
        )
        .unwrap();

        self.vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&mesh.vertices),
            label: Some("lyon vertex"),
            usage: wgpu::BufferUsages::VERTEX,
        });

        self.index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&mesh.indices),
            label: Some("lyon index"),
            usage: wgpu::BufferUsages::INDEX,
        });

        self.index_count = mesh.indices.len();
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        queue.write_buffer(
            &self.globals_buf,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32]),
        );
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>) {
        if self.index_count == 0 {
            return;
        }

        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_pipeline(&self.pipeline);
        rpass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        rpass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..self.index_count as u32, 0, 0..1);
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
struct Globals {
    window_size: [f32; 2],
    font_size: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct LyonVertex {
    position: [f32; 2],
}

struct VertexCtor {
    height: f32,
}

impl FillVertexConstructor<LyonVertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: lyon::lyon_tessellation::FillVertex) -> LyonVertex {
        let [x, y] = vertex.position().to_array();

        LyonVertex {
            position: [x / self.height, y / self.height],
        }
    }
}
