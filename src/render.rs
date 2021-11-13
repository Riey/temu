mod cell;
mod lyon;
mod viewport;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use self::{cell::CellContext, lyon::LyonContext};
use crate::term::{SharedTerminal, Terminal};
use crossbeam_channel::Receiver;
use futures_executor::{block_on, LocalPool, LocalSpawner};
use futures_task::{LocalFutureObj, LocalSpawn};
use temu_window::TemuEvent;
use ttf_parser::{Face, GlyphId};
use wgpu_glyph::{
    ab_glyph::{Font, FontRef, PxScale},
    GlyphBrush, GlyphBrushBuilder, Layout, Section, Text,
};

pub use self::viewport::Viewport;

const FONT: &[u8] = include_bytes!("../Hack Regular Nerd Font Complete Mono.ttf");


const FONT_SIZE: u32 = 18;

#[allow(unused)]
pub struct WgpuContext {
    viewport: Viewport,
    glyph: GlyphBrush<(), FontRef<'static>>,
    face: Face<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    cell_ctx: CellContext,
    lyon_ctx: LyonContext,
    staging_belt: wgpu::util::StagingBelt,
    scroll_state: ScrollState,
    terminal: Terminal,
    str_buf: String,
    font_width: u32,
}

impl WgpuContext {
    pub fn new(viewport: Viewport, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let mut scroll_state = ScrollState::new();
        scroll_state.page_size = 20;
        scroll_state.top = 10;
        scroll_state.max = 50;

        let face = Face::from_slice(FONT, 0).unwrap();

        let font = FontRef::try_from_slice(FONT).unwrap();
        let m_glyph = font.glyph_id('M');
        let font_width = font
            .glyph_bounds(&m_glyph.with_scale(PxScale::from(FONT_SIZE as f32)))
            .width() as u32;

        Self {
            cell_ctx: CellContext::new(&device, &viewport),
            lyon_ctx: LyonContext::new(&device, &viewport),
            face,
            glyph: GlyphBrushBuilder::using_font(FontRef::try_from_slice(FONT).unwrap())
                .build(&device, viewport.format()),
            viewport,
            staging_belt: wgpu::util::StagingBelt::new(1024),
            device,
            queue,
            scroll_state,
            terminal: Terminal::new(100),
            font_width,
            str_buf: String::new(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        log::info!("Resize({}, {})", width, height);

        self.cell_ctx.resize(&self.queue, width, height);
        self.viewport.resize(&self.device, width, height);
        // TODO: update scroll_state
    }

    pub fn redraw(&mut self, spawner: &LocalSpawner) {
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
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            self.cell_ctx.draw(&mut rpass);

            // let mut tess = FillTessellator::new();
            // let mut builder = LyonBuilder {
            //     builder: Builder::new(),
            // };
            // self.face.outline_glyph(GlyphId(0), &mut builder);
            // let mut mesh = VertexBuffers::<LyonVertex, u32>::new();
            // let path = builder.builder.build();
            // tess.tessellate_path(
            //     &path,
            //     &FillOptions::default(),
            //     &mut BuffersBuilder::new(&mut mesh, VertexCtor {}),
            // )
            // .unwrap();
        }

        // {
        //     let wgpu::Color { a, r, g, b } = self.viewport.foreground();
        //     let foreground = [a as f32, r as f32, g as f32, b as f32];
        //     let mut y = 0.0;

        //     let page_count = self.viewport.height() / FONT_SIZE;
        //     let start = self
        //         .terminal
        //         .rows()
        //         .len()
        //         .saturating_sub(page_count as usize);

        //     for row in self.terminal.rows().skip(start) {
        //         row.write_text(&mut self.str_buf);
        //         self.glyph.queue(Section {
        //             text: vec![Text::new(&self.str_buf)
        //                 .with_color(foreground)
        //                 .with_scale(PxScale::from(FONT_SIZE as f32))],
        //             screen_position: (0.0, y),
        //             layout: Layout::default_single_line(),
        //             ..Default::default()
        //         });
        //         self.str_buf.clear();

        //         y += FONT_SIZE as f32;
        //     }

        //     self.glyph
        //         .draw_queued(
        //             &self.device,
        //             &mut self.staging_belt,
        //             &mut encoder,
        //             &view,
        //             self.viewport.width(),
        //             self.viewport.height(),
        //         )
        //         .unwrap();
        // }

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
            limits: wgpu::Limits::downlevel_defaults(),
        },
        None,
    ))
    .expect("Failed to create device");

    let mut prev_resize = wait_size(&event_rx);

    let viewport = Viewport::new(prev_resize.0, prev_resize.1, &adapter, &device, surface);
    let mut ctx = WgpuContext::new(viewport, device, queue);
    let mut next_render_time = Instant::now();
    const FPS: u64 = 60;
    const FRAMETIME: Duration = Duration::from_millis(1000 / FPS);

    loop {
        if need_redraw {
            let now = Instant::now();

            if now < next_render_time {
                std::thread::sleep(next_render_time - now);
            }

            ctx.redraw(&local_spawner);
            local_pool.run_until_stalled();
            need_redraw = false;
            next_render_time = now + FRAMETIME;
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
}
