mod scroll;
// mod shm;

use ab_glyph::{Font, FontRef, PxScale, PxScaleFont, ScaleFont};
use crossbeam_channel::TryRecvError;
use glyph_brush_draw_cache::DrawCache;
use image::Rgba;
use smithay_client_toolkit as sctk;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use sctk::{
    environment::Environment,
    shm::{AutoMemPool, Format},
};
use sctk::{
    seat::SeatListener,
    window::{Event as WEvent, FallbackFrame, Window},
};

sctk::default_environment!(TemuEnv, desktop);

use crate::{
    event::{Rx, TemuEvent, Tx},
    term::{Cell, SharedTerminal, Terminal},
};
use wayland_client::{
    protocol::{
        wl_keyboard,
        wl_pointer::{self, Axis},
    },
    Display, EventQueue, Filter,
};

wayland_client::event_enum!(
    InputEvents |
    Pointer => wl_pointer::WlPointer,
    Keyboard => wl_keyboard::WlKeyboard
);

const FONT: &[u8] = include_bytes!("/nix/store/imnk1n6llkh089xgzqyqpr6yw9qz9b3z-d2codingfont-1.3.2/share/fonts/truetype/D2Coding-Ver1.3.2-20180524-all.ttc");
// const SHADER: &str = include_str!("../shaders/shader.wgsl");

pub struct WindowContext {
    env: Environment<TemuEnv>,
    window: Window<FallbackFrame>,
    queue: EventQueue,
    display: Display,
    pool: AutoMemPool,
    need_redraw: bool,
    prev_resize: (u32, u32),
    terminal: Terminal,
    event_rx: Rx,
    event_tx: Tx,
    shared_terminal: Arc<SharedTerminal>,
    glyph_cache: DrawCache,
    font: PxScaleFont<FontRef<'static>>,
    _seat_listener: SeatListener,
}

impl WindowContext {
    pub fn new(event_tx: Tx, event_rx: Rx, shared_terminal: Arc<SharedTerminal>) -> Self {
        let (env, display, mut queue) = sctk::new_default_environment!(TemuEnv, desktop).unwrap();
        let surface = env
            .create_surface_with_scale_callback(move |dpi, _surface, mut data| {
                if let Some(tx) = data.get::<Tx>() {
                    tx.send(TemuEvent::DpiChange { dpi }).ok();
                }
            })
            .detach();
        let mut window = env
            .create_window::<FallbackFrame, _>(surface, None, (300, 200), move |e, mut data| {
                let e = match e {
                    WEvent::Close => TemuEvent::Close,
                    WEvent::Configure {
                        new_size,
                        states: _,
                    } => match new_size {
                        Some((width, height)) => TemuEvent::Resize { width, height },
                        None => return,
                    },
                    WEvent::Refresh => TemuEvent::Redraw,
                };

                if let Some(tx) = data.get::<Tx>() {
                    tx.send(e).ok();
                }
            })
            .unwrap();

        window.set_title("Temu".into());

        let pool = env.create_auto_pool().unwrap();
        let loop_tx = event_tx.clone();

        #[allow(unused_variables)]
        let common_filter = Filter::new(move |event, _, _| match event {
            InputEvents::Pointer { event, .. } => match event {
                wl_pointer::Event::Enter {
                    surface_x,
                    surface_y,
                    ..
                } => {
                    // println!("Pointer entered at ({}, {}).", surface_x, surface_y);
                }
                wl_pointer::Event::Leave { .. } => {
                    // println!("Pointer left.");
                }
                wl_pointer::Event::Motion {
                    surface_x,
                    surface_y,
                    ..
                } => {
                    // println!("Pointer moved to ({}, {}).", surface_x, surface_y);
                }
                wl_pointer::Event::Button { button, state, .. } => {
                    loop_tx.send(TemuEvent::Redraw).ok();
                    // println!("Button {} was {:?}.", button, state);
                }
                wl_pointer::Event::Axis {
                    axis: Axis::VerticalScroll,
                    value,
                    ..
                } => {
                    if value > 0. {
                        loop_tx.send(TemuEvent::ScrollUp).ok();
                    } else if value < 0. {
                        loop_tx.send(TemuEvent::ScrollDown).ok();
                    }
                }
                _ => {}
            },
            InputEvents::Keyboard { event, .. } => match event {
                wl_keyboard::Event::Enter { .. } => {
                    // println!("Gained keyboard focus.");
                }
                wl_keyboard::Event::Leave { .. } => {
                    // println!("Lost keyboard focus.");
                }
                wl_keyboard::Event::Key { key, state, .. } => {
                    loop_tx.send(TemuEvent::Redraw).ok();
                    // println!("Key with id {} was {:?}.", key, state);
                }
                _ => (),
            },
        });
        // to be handled properly this should be more dynamic, as more
        // than one seat can exist (and they can be created and destroyed
        // dynamically), however most "traditional" setups have a single
        // seat, so we'll keep it simple here
        let mut pointer_created = false;
        let mut keyboard_created = false;

        let mut seats = HashMap::new();

        for seat in env.get_all_seats().into_iter() {
            if let Some((has_input, name)) = sctk::seat::with_seat_data(&seat, |seat_data| {
                (
                    (seat_data.has_keyboard && seat_data.has_pointer && !seat_data.defunct),
                    seat_data.name.clone(),
                )
            }) {
                if has_input {
                    let (kbd, pointer) = (seat.get_keyboard(), seat.get_pointer());
                    kbd.assign(common_filter.clone());
                    pointer.assign(common_filter.clone());
                    seats.insert(name, (kbd, pointer));
                }
            }
        }

        let seat_listener = env.listen_for_seats(move |seat, seat_data, _| {
            let has_input = seat_data.has_keyboard && seat_data.has_pointer && !seat_data.defunct;

            let mut entry = seats.entry(seat_data.name.clone());

            if has_input {
                entry.or_insert_with(|| {
                    let (kbd, pointer) = (seat.get_keyboard(), seat.get_pointer());
                    kbd.assign(common_filter.clone());
                    pointer.assign(common_filter.clone());
                    (kbd, pointer)
                });
            } else {
                entry.and_modify(|(kdb, pointer)| {
                    kdb.release();
                    pointer.release();
                });
                seats.remove(&seat_data.name);
            }
        });

        window.resize(300, 200);
        display.flush().unwrap();

        let font = FontRef::try_from_slice(FONT).unwrap();

        Self {
            display,
            pool,
            env,
            queue,
            window,
            prev_resize: (300, 200),
            terminal: Terminal::new(),
            need_redraw: true,
            event_rx,
            event_tx,
            shared_terminal,
            glyph_cache: DrawCache::builder().dimensions(300, 200).build(),
            font: font.into_scaled(PxScale::from(100.0)),
            _seat_listener: seat_listener,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.prev_resize.0 != width || self.prev_resize.1 != height {
            log::info!("Resize {}, {}", width, height);
            if width > self.glyph_cache.dimensions().0 || height > self.glyph_cache.dimensions().1 {
                self.glyph_cache = self
                    .glyph_cache
                    .to_builder()
                    .dimensions(width, height)
                    .build();
            }
            self.window.resize(width, height);
            self.window.refresh();
            self.need_redraw = true;
            self.prev_resize = (width, height);
        }
    }

    pub fn redraw(&mut self) {
        let start = Instant::now();

        self.need_redraw = false;

        let surface = self.window.surface();

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

    pub fn event_loop(&mut self) {
        log::debug!("Start event loop");

        if !self.env.get_shell().unwrap().needs_configure() {
            // initial draw to bootstrap on wl_shell
            self.redraw();
            self.window.refresh();
        }

        loop {
            let mut processed = self
                .queue
                .dispatch(&mut self.event_tx, |_, _, _| ())
                .unwrap();

            loop {
                match self.event_rx.try_recv() {
                    Ok(e) => {
                        processed += 1;

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
                processed += 1;
            }

            if processed > 0 {
                if self.need_redraw {
                    self.redraw();
                }
            } else {
                debug_assert!(!self.need_redraw);
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

pub fn run(
    event_tx: crossbeam_channel::Sender<TemuEvent>,
    event_rx: crossbeam_channel::Receiver<TemuEvent>,
    shared_terminal: Arc<SharedTerminal>,
) {
    let mut ctx = WindowContext::new(event_tx, event_rx, shared_terminal);

    log::debug!("Window initialized");

    ctx.event_loop();
}
