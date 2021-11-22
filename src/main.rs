mod render;
mod term;

use temu_window::{init_native_window, TemuWindow};

const COLUMN: u32 = 80;
const ROW: u32 = 23;

fn main() {
    profiling::register_thread!("Main Thread");
    let (event_tx, event_rx) = crossbeam_channel::bounded(64);

    env_logger::init();

    log::info!("Init window");
    let window = init_native_window(event_tx.clone());
    let (width, height) = window.size();
    let scale_factor = window.scale_factor();

    let instance = wgpu::Instance::new(wgpu::Backends::all());
    let surface = unsafe { instance.create_surface(&window) };

    std::thread::spawn(move || {
        render::run(instance, surface, width, height, scale_factor, event_rx);
    });

    log::info!("Start window");
    window.run();
}
