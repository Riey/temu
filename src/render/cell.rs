use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::Viewport;

pub struct CellContext {
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    window_size_buf: wgpu::Buffer,
}

impl CellContext {
    pub fn new(device: &wgpu::Device, viewport: &Viewport) -> Self {
        let shader = device.create_shader_module(&wgpu::include_wgsl!("../shaders/shader.wgsl"));

        // Create window size
        let window_size = WindowSize {
            size: [600.0, 400.0],
            cell_size: [20.0, super::FONT_SIZE as _],
            column: 5,
        };

        let window_size_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("size_bind_group_layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&window_size_bind_group_layout],
            push_constant_ranges: &[],
        });

        let window_size_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("WindowSize Buffer"),
            contents: bytemuck::cast_slice(&[window_size]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &window_size_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: window_size_buf.as_entire_binding(),
            }],
            label: Some("WindowSize bind_group"),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cell_pipeline"),
            layout: Some(&pipeline_layout),
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
                entry_point: "cell_fs",
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
            bind_group,
            instances,
            pipeline,
            window_size_buf,
        }
    }

    pub fn resize(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        // write window size
        queue.write_buffer(
            &self.window_size_buf,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32]),
        );
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>) {
        rpass.push_debug_group("Draw cell");
        rpass.set_bind_group(0, &self.bind_group, &[]);
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

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct WindowSize {
    size: [f32; 2],
    cell_size: [f32; 2],
    column: u32,
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
