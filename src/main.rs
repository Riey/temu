mod render;
mod term;

use crate::term::SharedTerminal;
use std::sync::Arc;
use temu_window::{init_native_window, TemuWindow};

fn main() {
    let (event_tx, event_rx) = crossbeam_channel::bounded(64);
    let (pty_event_tx, pty_event_rx) = crossbeam_channel::bounded(64);

    env_logger::init();

    let window = init_native_window(event_tx.clone(), pty_event_tx);
    let instance = wgpu::Instance::new(wgpu::Backends::all());
    let surface = unsafe { instance.create_surface(&window) };
    let shared = Arc::new(SharedTerminal::new());

    let shared_inner = shared.clone();
    std::thread::spawn(move || {
        render::run(instance, surface, event_rx, shared_inner);
    });

    std::thread::spawn(move || {
        term::run(event_tx, pty_event_rx, shared);
    });

    window.run();
}
