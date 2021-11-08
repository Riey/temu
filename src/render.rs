mod viewport;

use std::{sync::Arc, time::Instant};

use crate::{
    event::TemuEvent,
    term::{SharedTerminal, Terminal},
};
use bytemuck::{Pod, Zeroable};
use futures_executor::{block_on, LocalPool, LocalSpawner};
use futures_task::{LocalFutureObj, LocalSpawn};
use wgpu::util::DeviceExt;
use wgpu_glyph::{
    ab_glyph::{FontRef, PxScale, ScaleFont},
    GlyphBrush, GlyphBrushBuilder, Layout, Section, Text,
};

pub use self::viewport::{Viewport, ViewportDesc, WindowHandle};

const FONT: &[u8] = include_bytes!("/nix/store/lkc45rnr4dqq2ig9ahh817c31j4kxlnq-nerdfonts-2.1.0/share/fonts/truetype/NerdFonts/Hack Bold Nerd Font Complete.ttf");
const BAR_BG_COLOR: [f32; 3] = [0.5; 3];
const BAR_COLOR: [f32; 3] = [0.3; 3];
// const SHADER: &str = include_str!("../shaders/shader.wgsl");

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct WindowSize {
    size: [f32; 2],
}

const SCROLLBAR_INDICES: &[u16] = &[0, 1, 2, 1, 2, 3];
const FONT_SIZE: u32 = 18;

#[allow(unused)]
pub struct WgpuContext {
    viewport: Viewport,
    glyph: GlyphBrush<(), FontRef<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    staging_belt: wgpu::util::StagingBelt,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    window_size_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    inner_pipeline: wgpu::RenderPipeline,
    outter_pipeline: wgpu::RenderPipeline,
    scroll_state: ScrollState,
    terminal: Terminal,
    str_buf: Vec<u8>,
}

impl WgpuContext {
    pub fn new(viewport: Viewport, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let mut scroll_state = ScrollState::new();
        scroll_state.page_size = 20;
        scroll_state.top = 10;
        scroll_state.max = 50;

        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scrollbar Outter Vertex Buffer"),
            contents: bytemuck::cast_slice(
                &scroll_state
                    .calculate()
                    .get_vertexes(viewport.width(), viewport.height()),
            ),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(SCROLLBAR_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

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

        let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(
                std::fs::read_to_string("shaders/shader.wgsl")
                    .unwrap()
                    .into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&window_size_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create window size
        let window_size = WindowSize {
            size: [viewport.width() as _, viewport.height() as _],
        };

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

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as _,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x2,
                1 => Float32x3,
            ],
        }];

        let outter_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scrollbar_outter"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "rect_vs",
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "rect_fs",
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

        let inner_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scrollbar_inner"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "rect_vs",
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "rect_round_fs",
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

        Self {
            glyph: GlyphBrushBuilder::using_font(FontRef::try_from_slice(FONT).unwrap())
                .build(&device, viewport.format()),
            viewport,
            staging_belt: wgpu::util::StagingBelt::new(1024),
            device,
            queue,
            vertex_buf,
            inner_pipeline,
            outter_pipeline,
            index_buf,
            window_size_buf,
            bind_group,
            scroll_state,
            terminal: Terminal::new(100),
            str_buf: vec![0; 1024 * 16],
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        // TODO: update scroll_state
        self.viewport.resize(&self.device, width, height);
    }

    pub fn redraw(&mut self, spawner: &LocalSpawner) {
        let start = Instant::now();
        let frame = self.viewport.get_current_texture();
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            ..Default::default()
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("background"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.viewport.background()),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            rpass.set_bind_group(0, &self.bind_group, &[]);
            rpass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            rpass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);

            rpass.push_debug_group("Draw outter");
            rpass.set_pipeline(&self.outter_pipeline);
            rpass.draw_indexed(0..6, 0, 0..2);
            rpass.pop_debug_group();

            rpass.push_debug_group("Draw inner");
            rpass.set_pipeline(&self.inner_pipeline);
            rpass.draw_indexed(0..6, 4, 0..2);
            rpass.pop_debug_group();
        }

        {
            let wgpu::Color { a, r, g, b } = self.viewport.foreground();
            let foreground = [a as f32, r as f32, g as f32, b as f32];
            let mut y = 0.0;

            for row in self.terminal.rows() {
                let mut str_buf = &mut self.str_buf[..];
                let mut texts = Vec::with_capacity(row.len());
                for cell in row {
                    let len = cell.ch.len_utf8();
                    let (utf_8, left) = str_buf.split_at_mut(len);
                    str_buf = left;

                    texts.push(
                        Text::new(cell.ch.encode_utf8(utf_8))
                            .with_color(foreground)
                            .with_scale(PxScale::from(FONT_SIZE as f32)),
                    );
                }
                self.glyph.queue(Section {
                    text: texts,
                    screen_position: (0.0, y),
                    bounds: ((self.viewport.width() - 10) as f32, f32::INFINITY),
                    layout: Layout::default_single_line(),
                    ..Default::default()
                });

                y += FONT_SIZE as f32;
            }

            self.glyph
                .draw_queued(
                    &self.device,
                    &mut self.staging_belt,
                    &mut encoder,
                    &view,
                    self.viewport.width(),
                    self.viewport.height(),
                )
                .unwrap();
        }

        self.staging_belt.finish();
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        spawner
            .spawn_local_obj(LocalFutureObj::new(Box::new(self.staging_belt.recall())))
            .unwrap();

        let elapsed = Instant::now() - start;
        println!("Elapsed: {}ms", elapsed.as_millis());
    }
}

pub fn run(
    handle: WindowHandle,
    event_rx: crossbeam_channel::Receiver<TemuEvent>,
    shared_terminal: Arc<SharedTerminal>,
) {
    let mut local_pool = LocalPool::new();
    let local_spawner = local_pool.spawner();

    let instance = wgpu::Instance::new(wgpu::Backends::all());
    let viewport = ViewportDesc::new(&instance, &handle);
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(viewport.surface()),
        ..Default::default()
    }))
    .expect("Failed to find an appropriate adapter");

    let (device, queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::downlevel_defaults(),
        },
        None,
    ))
    .expect("Failed to create device");

    let mut ctx = WgpuContext::new(viewport.build(300, 200, &adapter, &device), device, queue);

    let mut need_redraw = true;
    let mut prev_resize = (300, 200);

    loop {
        if need_redraw {
            ctx.redraw(&local_spawner);
            local_pool.run_until_stalled();
            need_redraw = false;
        }

        if let Some(terminal) = shared_terminal.take_terminal() {
            ctx.terminal = terminal;
            need_redraw = true;
        }

        match event_rx.try_recv() {
            Ok(event) => match event {
                TemuEvent::Close => {
                    break;
                }
                TemuEvent::Resize { width, height } => {
                    if prev_resize != (width, height) {
                        ctx.resize(width, height);
                        need_redraw = true;
                        prev_resize = (width, height);
                    }
                }
                TemuEvent::Redraw => {
                    need_redraw = true;
                }
                TemuEvent::ScrollUp => {}
                TemuEvent::ScrollDown => {}
            },
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                break;
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

struct ScrollState {
    top: u32,
    max: u32,
    page_size: u32,
}

struct ScrollCalcResult {
    top: f32,
    bottom: f32,
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            top: 0,
            max: 1,
            page_size: 1,
        }
    }

    pub fn calculate(&self) -> ScrollCalcResult {
        match self.max.checked_sub(self.top) {
            None => ScrollCalcResult::FULL,
            Some(left) => ScrollCalcResult {
                top: self.top as f32 / self.max as f32,
                bottom: left as f32 / self.max as f32,
            },
        }
    }
}

impl ScrollCalcResult {
    /// Can display all lines
    const FULL: Self = ScrollCalcResult {
        top: 0.0,
        bottom: 1.0,
    };

    pub fn get_vertexes(&self, width: u32, height: u32) -> [Vertex; 8] {
        let width = width as f32;
        let height = height as f32;

        let left = 1.0 - (10.0 / width);
        let margin_left = 2.5 / width;
        let margin_top = 2.0 / height;

        let inner_top = (1.0 - margin_top * 2.0) * self.top;
        let inner_bottom = (1.0 - margin_top * 2.0) * self.bottom;

        [
            Vertex {
                position: [left, 1.0],
                color: BAR_BG_COLOR,
            },
            Vertex {
                position: [1.0, 1.0],
                color: BAR_BG_COLOR,
            },
            Vertex {
                position: [left, -1.0],
                color: BAR_BG_COLOR,
            },
            Vertex {
                position: [1.0, -1.0],
                color: BAR_BG_COLOR,
            },
            Vertex {
                position: [left + margin_left, inner_top],
                color: BAR_COLOR,
            },
            Vertex {
                position: [1.0 - margin_left, inner_top],
                color: BAR_COLOR,
            },
            Vertex {
                position: [left + margin_left, inner_bottom],
                color: BAR_COLOR,
            },
            Vertex {
                position: [1.0 - margin_left, inner_bottom],
                color: BAR_COLOR,
            },
        ]
    }
}
