mod mempool;

use self::mempool::AutoMemPool;

use crate::{
    event::{Rx, TemuEvent, Tx},
    term::{Cell, SharedTerminal, Terminal},
};

use ab_glyph::{Font, FontRef, PxScale, PxScaleFont, ScaleFont};
use crossbeam_channel::TryRecvError;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use wayland_client::{
    protocol::{
        wl_keyboard,
        wl_pointer::{self, Axis},
        wl_shm::{self, Format},
        wl_surface,
    },
    Display, GlobalManager,
};

wayland_client::event_enum!(
    InputEvents |
    Pointer => wl_pointer::WlPointer,
    Keyboard => wl_keyboard::WlKeyboard
);

const FONT: &[u8] = include_bytes!("/nix/store/imnk1n6llkh089xgzqyqpr6yw9qz9b3z-d2codingfont-1.3.2/share/fonts/truetype/D2Coding-Ver1.3.2-20180524-all.ttc");
// const SHADER: &str = include_str!("../shaders/shader.wgsl");

pub struct WindowContext {
    display: Display,
    pool: AutoMemPool,
    surface: wl_surface::WlSurface,
    need_redraw: bool,
    prev_resize: (u32, u32),
    terminal: Terminal,
    event_rx: Rx,
    event_tx: Tx,
    shared_terminal: Arc<SharedTerminal>,
    font: PxScaleFont<FontRef<'static>>,
}

impl WindowContext {
    pub fn new(
        event_tx: Tx,
        event_rx: Rx,
        shared_terminal: Arc<SharedTerminal>,
        display: Display,
        surface: wl_surface::WlSurface,
    ) -> Self {
        let mut event_queue = display.create_event_queue();

        let attached_display = (*display).clone().attach(event_queue.token());

        let globals = GlobalManager::new(&attached_display);

        // Make a synchronized roundtrip to the wayland server.
        //
        // When this returns it must be true that the server has already
        // sent us all available globals.
        event_queue
            .sync_roundtrip(&mut (), |_, _, _| unreachable!())
            .unwrap();

        let shm = globals.instantiate_exact::<wl_shm::WlShm>(1).unwrap();
        let pool = AutoMemPool::new(shm.into()).unwrap();

        event_queue.sync_roundtrip(&mut (), |_, _, _| ()).unwrap();

        display.flush().unwrap();

        let font = FontRef::try_from_slice(FONT).unwrap();

        Self {
            display,
            pool,
            surface,
            prev_resize: (300, 200),
            terminal: Terminal::new(),
            need_redraw: true,
            event_rx,
            event_tx,
            shared_terminal,
            font: font.into_scaled(PxScale::from(100.0)),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.prev_resize.0 != width || self.prev_resize.1 != height {
            log::info!("Resize {}, {}", width, height);
            self.need_redraw = true;
            self.prev_resize = (width, height);
        }
    }

    pub fn redraw(&mut self) {
        let start = Instant::now();

        self.need_redraw = false;

        let surface = &self.surface;

        let (width, height) = self.prev_resize;
        let (canvas, buffer) = self
            .pool
            .buffer(
                width as i32,
                height as i32,
                4 * width as i32,
                Format::Argb8888,
            )
            .unwrap();

        // Fill with Black
        let canvas: &mut [u32] = bytemuck::cast_slice_mut(canvas);
        canvas.fill(0xFF_00_00_00_u32);

        let text = "가나다";

        let mut base_x = 0;
        for ch in text.chars() {
            let glyph = self.font.scaled_glyph(ch);
            let outline = self.font.outline_glyph(glyph).unwrap();
            {
                let base_x = base_x + outline.px_bounds().min.x as u32;
                outline.draw(|x, y, p| {
                    if p < 0.0002 {
                        return;
                    }
                    let alpha = (p * 255.0) as u8;
                    canvas[(y * width + x + base_x) as usize] =
                        u32::from_be_bytes([255, alpha, alpha, alpha]);
                });
            }
            base_x += outline.px_bounds().max.x as u32;
        }

        surface.attach(Some(&buffer), 0, 0);
        // damage the surface so that the compositor knows it needs to redraw it
        if surface.as_ref().version() >= 4 {
            // If our server is recent enough and supports at least version 4 of the
            // wl_surface interface, we can specify the damage in buffer coordinates.
            // This is obviously the best and do that if possible.
            surface.damage_buffer(0, 0, width as i32, height as i32);
        } else {
            // Otherwise, we fallback to compatilibity mode. Here we specify damage
            // in surface coordinates, which would have been different if we had drawn
            // our buffer at HiDPI resolution. We didn't though, so it is ok.
            // Using `damage_buffer` in general is better though.
            surface.damage(0, 0, width as i32, height as i32);
        }
        surface.commit();

        let end = Instant::now();
        log::debug!("Elapsed: {}ms", (end - start).as_millis());
    }

    pub fn run(&mut self) {
        loop {
            loop {
                match self.event_rx.try_recv() {
                    Ok(e) => {
                        match e {
                            TemuEvent::Close => return,
                            TemuEvent::Redraw => self.need_redraw = true,
                            TemuEvent::Resize { width, height } => self.resize(width, height),
                            // TODO
                            TemuEvent::DpiChange { dpi } => {}
                            TemuEvent::ScrollUp | TemuEvent::ScrollDown => {}
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        return;
                    }
                }
            }

            if let Some(terminal) = self.shared_terminal.take_terminal() {
                self.terminal = terminal;
                self.need_redraw = true;
            }

            if self.need_redraw {
                self.redraw();
            } else {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

pub fn run(
    event_tx: Tx,
    event_rx: Rx,
    shared_terminal: Arc<SharedTerminal>,
    display: Display,
    surface: wl_surface::WlSurface,
) {
    let mut ctx = WindowContext::new(event_tx, event_rx, shared_terminal, display, surface);

    log::debug!("Window initialized");

    ctx.run();
}
