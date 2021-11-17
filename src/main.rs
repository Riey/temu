mod render;
mod term;

use temu_window::{init_native_window, TemuWindow};

fn main() {
    let (event_tx, event_rx) = crossbeam_channel::bounded(64);
    let (pty_event_tx, pty_event_rx) = crossbeam_channel::bounded(64);
    let (term_tx, term_rx) = crossbeam_channel::bounded(64);

    env_logger::init();

    log::info!("Init window");
    let window = init_native_window(event_tx.clone(), pty_event_tx);
    let (width, height) = window.size();
    let scale_factor = window.scale_factor();

    let instance = wgpu::Instance::new(wgpu::Backends::all());
    let surface = unsafe { instance.create_surface(&window) };

    std::thread::spawn(move || {
        render::run(
            instance,
            surface,
            width,
            height,
            scale_factor,
            event_rx,
            term_rx,
        );
    });

    std::thread::spawn(move || {
        term::run(event_tx, pty_event_rx, term_tx);
    });

    log::info!("Start window");
    window.run();
}
