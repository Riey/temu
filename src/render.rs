mod atlas;
mod cell;
mod font_texture;
mod viewport;

use std::{io::Write, sync::Arc, time::Instant};

pub use self::viewport::Viewport;
use self::{
    cell::CellContext,
    font_texture::{FontTexture, GlyphCacheInfo},
};
use crossbeam_channel::Receiver;
use futures_executor::block_on;
use temu_window::TemuEvent;
use termwiz::escape::Action;
use wezterm_term::{KeyCode, Terminal, TerminalSize};

const FONT: &[u8] = include_bytes!("../Hack Regular Nerd Font Complete Mono.ttf");

const FONT_SIZE: f32 = 15.0;
const TEXTURE_WIDTH: u32 = 1024;
const TEXTURE_SIZE: usize = (TEXTURE_WIDTH * TEXTURE_WIDTH) as usize;

#[allow(unused)]
pub struct WgpuContext {
    viewport: Viewport,
    device: wgpu::Device,
    queue: wgpu::Queue,
    cell_ctx: CellContext,
    str_buf: String,
}

impl WgpuContext {
    pub fn new(
        viewport: Viewport,
        device: wgpu::Device,
        queue: wgpu::Queue,
        font_texture: FontTexture,
        scale_factor: f32,
    ) -> Self {
        let cell_ctx = CellContext::new(
            &device,
            &queue,
            &viewport,
            font_texture,
            FONT_SIZE,
            scale_factor,
        );

        Self {
            cell_ctx,
            viewport,
            device,
            queue,
            str_buf: String::new(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        log::trace!("Resize({}, {})", width, height);

        self.viewport.resize(&self.device, width, height);
        self.cell_ctx.resize(width as _, height as _);
        // TODO: update scroll_state
    }

    #[profiling::function]
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
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: crate::DEFAULT_BG[0] as _,
                            g: crate::DEFAULT_BG[1] as _,
                            b: crate::DEFAULT_BG[2] as _,
                            a: crate::DEFAULT_BG[3] as _,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            self.cell_ctx.draw(&self.queue, &mut rpass);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();

        let end = start.elapsed();

        log::debug!("Redraw elapsed: {}us", end.as_micros());
    }
}

#[profiling::function]
pub fn generate_font_texture(scale_factor: f32) -> FontTexture {
    FontTexture::new(
        swash::FontRef::from_index(FONT, 0).unwrap(),
        FONT_SIZE * scale_factor,
    )
}

pub fn run(
    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    font_texture: FontTexture,
    width: u32,
    height: u32,
    scale_factor: f32,
    event_rx: Receiver<TemuEvent>,
    msg_rx: Receiver<Vec<Action>>,
    output: Box<dyn Write + Send>,
) {
    profiling::register_thread!("Renderer");

    let mut terminal = Terminal::new(
        TerminalSize {
            physical_cols: crate::COLUMN as _,
            physical_rows: crate::ROW as _,
            pixel_height: 0,
            pixel_width: 0,
        },
        Arc::new(crate::term::TerminalConfig),
        "temu",
        "0.1.0",
        output,
    );

    let mut need_redraw = true;

    let (device, queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::downlevel_defaults(),
        },
        None,
    ))
    .expect("Failed to create device");

    let mut current_size = (width, height);

    let viewport = Viewport::new(current_size.0, current_size.1, &adapter, &device, surface);
    let mut ctx = WgpuContext::new(viewport, device, queue, font_texture, scale_factor);
    // let mut fps = fps_counter::FPSCounter::new();
    // let mut fps_showtime = Instant::now();
    let always_redraw = false;
    let mut cursor_pos = (0.0, 0.0);
    let mut pressed = false;
    let mut dragged = false;

    loop {
        profiling::scope!("Render loop");

        crossbeam_channel::select! {
            recv(msg_rx) -> actions => {
                profiling::scope!("Process actions");
                terminal.perform_actions(actions.unwrap());
                ctx.cell_ctx.scroll_to_bottom(&terminal);
                ctx.cell_ctx
                    .set_terminal(&ctx.device, &ctx.queue, &terminal);
                need_redraw = true;
            }
            recv(event_rx) -> event => {
                match event.unwrap() {
                    TemuEvent::Char(c) => {
                        terminal
                            .key_down(KeyCode::Char(c), Default::default())
                            .unwrap();
                    }
                    TemuEvent::Close => {
                        break;
                    }
                    TemuEvent::Resize { width, height } => {
                        if width == 0 || height == 0 {
                            continue;
                        }
                        if current_size != (width, height) {
                            ctx.resize(width, height);
                            // need_redraw = true;
                            current_size = (width, height);
                        }
                    }
                    TemuEvent::CursorMove { x, y } => {
                        if pressed {
                            if ctx.cell_ctx.drag(x, y) {
                                need_redraw = true;
                            }
                            dragged = true;
                        } else {
                            if ctx.cell_ctx.hover(x, y) {
                                need_redraw = true;
                            }
                        }

                        cursor_pos = (x, y);
                    }
                    TemuEvent::Left(true) => {
                        pressed = true;
                    }
                    TemuEvent::Left(false) => {
                        if dragged {
                            ctx.cell_ctx.drag_end();
                        } else {
                            ctx.cell_ctx.click(cursor_pos.0, cursor_pos.1);
                        }
                        need_redraw = true;
                        dragged = false;
                        pressed = false;
                    }
                    TemuEvent::Redraw => {
                        need_redraw = true;
                    }
                    TemuEvent::ScrollUp => {
                        ctx.cell_ctx.scroll(-1, &terminal);
                        ctx.cell_ctx
                            .set_terminal(&ctx.device, &ctx.queue, &terminal);
                        need_redraw = true;
                    }
                    TemuEvent::ScrollDown => {
                        ctx.cell_ctx.scroll(1, &terminal);
                        ctx.cell_ctx
                            .set_terminal(&ctx.device, &ctx.queue, &terminal);
                        need_redraw = true;
                    }
                }
            }
        };

        if always_redraw || need_redraw {
            ctx.redraw();
            // let cur_fps = fps.tick();
            // let now = Instant::now();
            // if now > fps_showtime {
            //     fps_showtime = now + Duration::from_secs(1);
            //     println!("{}FPS", cur_fps);
            // }
            need_redraw = always_redraw;
        }

        profiling::finish_frame!();
    }
}
