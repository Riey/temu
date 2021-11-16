mod cell;
mod lyon;
mod viewport;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub use self::viewport::Viewport;
use self::{cell::CellContext, lyon::LyonContext};
use crate::term::{SharedTerminal, Terminal};
use crossbeam_channel::Receiver;
use futures_executor::{block_on, LocalPool, LocalSpawner};
use futures_task::{LocalFutureObj, LocalSpawn};
use temu_window::TemuEvent;
use wgpu::util::DeviceExt;

const FONT: &[u8] = include_bytes!("../Hack Regular Nerd Font Complete Mono.ttf");
const SAMPLE_COUNT: u32 = 4;
const FONT_SIZE: u32 = 300;

#[allow(unused)]
pub struct WgpuContext {
    viewport: Viewport,
    device: wgpu::Device,
    queue: wgpu::Queue,
    cell_ctx: CellContext,
    lyon_ctx: LyonContext,
    staging_belt: wgpu::util::StagingBelt,
    window_size_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    scroll_state: ScrollState,
    terminal: Terminal,
    str_buf: String,
    msaa: wgpu::TextureView,
    next_resize: Option<(u32, u32)>,
}

impl WgpuContext {
    pub fn new(viewport: Viewport, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let mut scroll_state = ScrollState::new();
        scroll_state.page_size = 20;
        scroll_state.top = 10;
        scroll_state.max = 50;

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(&wgpu::include_wgsl!("shaders/shader.wgsl"));

        let lyon_ctx = LyonContext::new(
            &device,
            &shader,
            &pipeline_layout,
            &viewport,
            FONT_SIZE as _,
        );
        let cell_size = [lyon_ctx.font_width(), lyon_ctx.font_height()];

        // Create window size
        let window_size = WindowSize {
            size: [viewport.width() as _, viewport.height() as _],
            cell_size,
            column: 5,
        };

        let window_size_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&[window_size]),
            label: Some("window size buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("window size bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(window_size_buf.as_entire_buffer_binding()),
            }],
        });

        Self {
            msaa: create_msaa_texture(
                &device,
                viewport.format(),
                viewport.width(),
                viewport.height(),
            ),
            lyon_ctx,
            cell_ctx: CellContext::new(&device, &viewport, &pipeline_layout),
            window_size_buf,
            bind_group,
            viewport,
            staging_belt: wgpu::util::StagingBelt::new(1024),
            device,
            queue,
            next_resize: None,
            scroll_state,
            terminal: Terminal::new(100),
            str_buf: String::new(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        // TODO: update scroll_state

        // lazy update
        self.next_resize = Some((width, height));
    }

    pub fn redraw(&mut self, spawner: &LocalSpawner) {
        if let Some((width, height)) = self.next_resize.take() {
            self.msaa = create_msaa_texture(&self.device, self.viewport.format(), width, height);
            self.queue.write_buffer(
                &self.window_size_buf,
                0,
                bytemuck::cast_slice(&[width as f32, height as f32]),
            );
            self.viewport.resize(&self.device, width, height);
        }

        let frame = match self.viewport.get_current_texture() {
            Some(frame) => frame,
            None => return,
        };

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("background"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &self.msaa,
                    resolve_target: Some(&view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            rpass.set_bind_group(0, &self.bind_group, &[]);
            self.cell_ctx.draw(&mut rpass);
            self.lyon_ctx.draw(&mut rpass);
        }

        self.staging_belt.finish();
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        spawner
            .spawn_local_obj(LocalFutureObj::new(Box::new(self.staging_belt.recall())))
            .unwrap();
    }
}

fn wait_size(event_rx: &Receiver<TemuEvent>) -> (u32, u32) {
    loop {
        let e = event_rx.recv().unwrap();
        match e {
            TemuEvent::Resize { width, height } => {
                return (width, height);
            }
            _ => {}
        }
    }
}

pub fn run(
    instance: wgpu::Instance,
    surface: wgpu::Surface,
    event_rx: Receiver<TemuEvent>,
    shared_terminal: Arc<SharedTerminal>,
) {
    let mut local_pool = LocalPool::new();
    let local_spawner = local_pool.spawner();

    let mut need_redraw = true;

    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..Default::default()
    }))
    .expect("Failed to find an appropriate adapter");

    let (device, queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
        },
        None,
    ))
    .expect("Failed to create device");

    let mut prev_resize = wait_size(&event_rx);

    let viewport = Viewport::new(prev_resize.0, prev_resize.1, &adapter, &device, surface);
    let mut ctx = WgpuContext::new(viewport, device, queue);
    ctx.lyon_ctx.set_draw(&ctx.device, "Hello, world!");
    let mut next_render_time = Instant::now();
    const FPS: u64 = 60;
    const FRAMETIME: Duration = Duration::from_millis(1000 / FPS);

    loop {
        if need_redraw {
            let now = Instant::now();

            if now >= next_render_time {
                ctx.redraw(&local_spawner);
                local_pool.run_until_stalled();
                need_redraw = false;
                next_render_time = now + FRAMETIME;
            }
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
                    if width == 0 || height == 0 {
                        continue;
                    }
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
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WindowSize {
    size: [f32; 2],
    cell_size: [f32; 2],
    column: u32,
}

fn create_msaa_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa"),
            format,
            dimension: wgpu::TextureDimension::D2,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}
