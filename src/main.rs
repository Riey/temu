mod event;
mod render;
mod term;

use crate::{event::TemuEvent, term::SharedTerminal};
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
    let (event_tx, event_rx) = self::event::channel();

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
            if width == 0 || height == 0 {
                return;
            }

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
                tx.send(TemuEvent::Redraw).ok();
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
            wl_keyboard::Event::Key { key, state, .. } => {
                tx.send(TemuEvent::Redraw).ok();
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

    let surface = surface.detach();

    let shared = Arc::new(SharedTerminal::new());

    let shared_inner = shared.clone();
    let tx_inner = event_tx.clone();
    std::thread::spawn(move || {
        render::run(tx_inner, event_rx, shared_inner, display, surface);
    });

    std::thread::spawn(move || {
        term::run(event_tx, shared);
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
