mod atals;
mod cell;
mod viewport;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use self::cell::CellContext;
pub use self::viewport::Viewport;
use crate::term::SharedTerminal;
use crossbeam_channel::Receiver;
use futures_executor::block_on;
use temu_window::TemuEvent;

const FONT: &[u8] = include_bytes!("../Hack Regular Nerd Font Complete Mono.ttf");

const FONT_SIZE: f32 = 15.0;

#[allow(unused)]
pub struct WgpuContext {
    viewport: Viewport,
    device: wgpu::Device,
    queue: wgpu::Queue,
    cell_ctx: CellContext,
    scroll_state: ScrollState,
    str_buf: String,
}

impl WgpuContext {
    pub fn new(
        viewport: Viewport,
        device: wgpu::Device,
        queue: wgpu::Queue,
        scale_factor: f32,
    ) -> Self {
        let mut scroll_state = ScrollState::new();
        scroll_state.page_size = 20;
        scroll_state.top = 10;
        scroll_state.max = 100;

        let cell_ctx = CellContext::new(&device, &queue, &viewport, FONT_SIZE * scale_factor);

        Self {
            cell_ctx,
            viewport,
            device,
            queue,
            scroll_state,
            str_buf: String::new(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        log::info!("Resize({}, {})", width, height);

        self.cell_ctx.resize(&self.queue, width, height);
        self.viewport.resize(&self.device, width, height);
        // TODO: update scroll_state
    }

    pub fn redraw(&mut self) {
        let start = Instant::now();

        let frame = match self.viewport.get_current_texture() {
            Some(frame) => frame,
            None => return,
        };

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

            self.cell_ctx.draw(&mut rpass);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();

        let end = start.elapsed();

        log::info!("Redraw elapsed: {}us", end.as_micros());
    }
}

pub fn run(
    instance: wgpu::Instance,
    surface: wgpu::Surface,
    width: u32,
    height: u32,
    scale_factor: f32,
    event_rx: Receiver<TemuEvent>,
    shared_terminal: Arc<SharedTerminal>,
) {
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
            limits: wgpu::Limits::downlevel_defaults(),
        },
        None,
    ))
    .expect("Failed to create device");

    let mut prev_resize = (width, height);

    let viewport = Viewport::new(prev_resize.0, prev_resize.1, &adapter, &device, surface);
    let mut ctx = WgpuContext::new(viewport, device, queue, scale_factor);

    loop {
        if need_redraw {
            ctx.redraw();
            need_redraw = false;
        }

        if let Some(terminal) = shared_terminal.take_terminal() {
            ctx.cell_ctx
                .set_terminal(&ctx.device, &ctx.queue, &terminal);
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
                if !need_redraw {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
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
