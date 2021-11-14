use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::Viewport;

pub struct CellContext {
    pipeline: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
}

impl CellContext {
    pub fn new(
        device: &wgpu::Device,
        viewport: &Viewport,
        pipeline_layout: &wgpu::PipelineLayout,
    ) -> Self {
        let shader = device.create_shader_module(&wgpu::include_wgsl!("../shaders/shader.wgsl"));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cell_pipeline"),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "cell_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as _,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Uint32,
                        1 => Float32x4,
                        2 => Float32x4,
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "simple_fs",
                targets: &[viewport.format().into()],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                ..Default::default()
            },
        });

        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cell Vertex Buffer"),
            contents: bytemuck::cast_slice(&create_cell_instance(5, 10)),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            instances,
            pipeline,
        }
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>) {
        rpass.push_debug_group("Draw cell");
        rpass.set_pipeline(&self.pipeline);
        rpass.set_vertex_buffer(0, self.instances.slice(..));
        rpass.draw(0..4, 0..15);
        rpass.pop_debug_group();
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Vertex {
    cell_index: u32,
    color: [f32; 4],
    bg_color: [f32; 4],
}

fn create_cell_instance(column: u32, row: u32) -> Vec<Vertex> {
    (0..(column * row))
        .map(|index| Vertex {
            cell_index: index,
            color: [1.0, 1.0, 1.0, 1.0],
            bg_color: [0.0, 0.0, 0.0, 1.0],
        })
        .collect()
}
