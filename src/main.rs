mod event;
mod render;
mod term;

use crate::event::TemuPtyEvent;
use crate::render::WindowHandle;
use crate::{event::TemuEvent, term::SharedTerminal};
use kime_engine_core::{InputEngine, InputResult, KeyCode, ModifierState};
use std::{convert::TryInto, sync::Arc};

use wayland_client::{
    event_enum,
    protocol::{
        wl_compositor, wl_keyboard,
        wl_pointer::{self, Axis},
        wl_seat,
    },
    Display, Filter, GlobalManager,
};
use wayland_protocols::xdg_shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

// declare an event enum containing the events we want to receive in the iterator
event_enum!(
    Events |
    Pointer => wl_pointer::WlPointer,
    Keyboard => wl_keyboard::WlKeyboard
);

fn main() {
    let (event_tx, event_rx) = crossbeam_channel::bounded(64);
    let (pty_event_ty, pty_event_rx) = crossbeam_channel::bounded(64);

    env_logger::init();

    let display = Display::connect_to_env().unwrap();

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

    /*
     * Init wayland objects
     */

    // The compositor allows us to creates surfaces
    let compositor = globals
        .instantiate_exact::<wl_compositor::WlCompositor>(1)
        .unwrap();
    let surface = compositor.create_surface();

    let handle = WindowHandle::new(&surface, &display);

    let xdg_wm_base = globals
        .instantiate_exact::<xdg_wm_base::XdgWmBase>(2)
        .expect("Compositor does not support xdg_shell");

    xdg_wm_base.quick_assign(|xdg_wm_base, event, _| {
        if let xdg_wm_base::Event::Ping { serial } = event {
            xdg_wm_base.pong(serial);
        };
    });

    let xdg_surface = xdg_wm_base.get_xdg_surface(&surface);
    xdg_surface.quick_assign(move |xdg_surface, event, _| match event {
        xdg_surface::Event::Configure { serial } => {
            xdg_surface.ack_configure(serial);
        }
        _ => unreachable!(),
    });

    let tx = event_tx.clone();
    let xdg_toplevel = xdg_surface.get_toplevel();
    xdg_toplevel.quick_assign(move |_, event, mut data| match event {
        xdg_toplevel::Event::Close => {
            log::info!("Closed");
            *data.get().unwrap() = true;
            tx.send(TemuEvent::Close).ok();
        }
        xdg_toplevel::Event::Configure {
            width,
            height,
            states,
        } => {
            let states = states
                .windows(4)
                .filter_map(|state| {
                    xdg_toplevel::State::from_raw(u32::from_ne_bytes(state.try_into().ok()?))
                })
                .collect::<Vec<_>>();

            tx.send(TemuEvent::Resize {
                width: width as u32,
                height: height as u32,
            })
            .ok();
        }
        _ => unreachable!(),
    });
    xdg_toplevel.set_title("Temu".to_string());

    let mut config = kime_engine_core::config_load_from_config_dir().unwrap().0;
    // commit english
    config.preferred_direct = false;

    let mut engine = InputEngine::new(&config);
    let mut modifier = ModifierState::empty();

    let tx = event_tx.clone();
    // initialize a seat to retrieve pointer & keyboard events
    //
    // example of using a common filter to handle both pointer & keyboard events
    #[allow(unused_variables)]
    let common_filter = Filter::new(move |event, _, _| match event {
        Events::Pointer { event, .. } => match event {
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
                // println!("Button {} was {:?}.", button, state);
            }
            wl_pointer::Event::Axis {
                axis: Axis::VerticalScroll,
                value,
                ..
            } => {
                if value > 0. {
                    tx.send(TemuEvent::ScrollUp).ok();
                } else if value < 0. {
                    tx.send(TemuEvent::ScrollDown).ok();
                }
            }
            _ => {}
        },
        Events::Keyboard { event, .. } => match event {
            wl_keyboard::Event::Enter { .. } => {
                // println!("Gained keyboard focus.");
            }
            wl_keyboard::Event::Leave { .. } => {
                // println!("Lost keyboard focus.");
            }
            wl_keyboard::Event::Keymap { fd, .. } => unsafe {
                libc::close(fd);
            },
            wl_keyboard::Event::Modifiers { mods_depressed, .. } => {
                if mods_depressed & 0x1 != 0 {
                    modifier.insert(ModifierState::SHIFT);
                }
                if mods_depressed & 0x4 != 0 {
                    modifier.insert(ModifierState::CONTROL);
                }
                if mods_depressed & 0x8 != 0 {
                    modifier.insert(ModifierState::ALT);
                }
                if mods_depressed & 0x40 != 0 {
                    modifier.insert(ModifierState::SUPER);
                }
            }
            wl_keyboard::Event::Key { key, state, .. } => {
                if state == wl_keyboard::KeyState::Released {
                    return;
                }

                let ret = engine.press_key_code((key + 8) as u16, modifier, &config);

                log::debug!("ret: {:?}", ret);

                // TODO: preedit, not_ready

                if ret.contains(InputResult::LANGUAGE_CHANGED) {
                    engine.update_layout_state().ok();
                }

                if ret.contains(InputResult::HAS_COMMIT) {
                    let commit = engine.commit_str();
                    pty_event_ty.send(TemuPtyEvent::Text(commit.into())).ok();
                    engine.clear_commit();
                }

                let bypassed = !ret.contains(InputResult::CONSUMED);

                if bypassed {
                    match KeyCode::from_hardward_code((key as u16) + 8) {
                        Some(KeyCode::Enter) => {
                            pty_event_ty.send(TemuPtyEvent::Enter).ok();
                        }
                        _ => {}
                    }
                }

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
    globals
        .instantiate_exact::<wl_seat::WlSeat>(1)
        .unwrap()
        .quick_assign(move |seat, event, _| {
            // The capabilities of a seat are known at runtime and we retrieve
            // them via an events. 3 capabilities exists: pointer, keyboard, and touch
            // we are only interested in pointer & keyboard here
            use wayland_client::protocol::wl_seat::{Capability, Event as SeatEvent};

            if let SeatEvent::Capabilities { capabilities } = event {
                if !pointer_created && capabilities.contains(Capability::Pointer) {
                    // create the pointer only once
                    pointer_created = true;
                    seat.get_pointer().assign(common_filter.clone());
                }
                if !keyboard_created && capabilities.contains(Capability::Keyboard) {
                    // create the keyboard only once
                    keyboard_created = true;
                    seat.get_keyboard().assign(common_filter.clone());
                }
            }
        });

    surface.commit();

    let shared = Arc::new(SharedTerminal::new());

    let shared_inner = shared.clone();
    std::thread::spawn(move || {
        render::run(handle, event_rx, shared_inner);
    });

    std::thread::spawn(move || {
        term::run(event_tx, pty_event_rx, shared);
    });

    let mut closed = false;

    loop {
        event_queue
            .sync_roundtrip(&mut closed, |_, _, _| { /* we ignore unfiltered messages */
            })
            .unwrap();
        if closed {
            break;
        } else {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
