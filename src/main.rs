#![windows_subsystem = "windows"]

mod render;
mod term;

use std::io::{BufReader, Read};

use crossbeam_channel::Receiver;
use temu_window::{init_native_window, TemuWindow};
use termwiz::escape::{parser::Parser, Action};

const COLUMN: u32 = 80;
const ROW: u32 = 23;
const DEFAULT_BG: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const DEFAULT_TEXT: [f32; 3] = [1.0, 1.0, 1.0];

fn main() {
    profiling::register_thread!("Main Thread");

    let adapter_handle = std::thread::spawn(|| {
        profiling::register_thread!("Init Adapter Thread");
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let adapters: Vec<_> = instance.enumerate_adapters(wgpu::Backends::all()).collect();

        (instance, adapters)
    });

    let pty_handle = std::thread::spawn(|| {
        profiling::register_thread!("Init Pty Thread");
        let (master, shell) = crate::term::start_pty();
        let input = master.try_clone_reader().unwrap();

        let msg_rx = run_reader(input);
        let output = master.try_clone_writer().unwrap();

        (output, master, shell, msg_rx)
    });

    let (event_tx, event_rx) = crossbeam_channel::bounded(64);

    env_logger::init();

    log::info!("Init window");
    let window = init_native_window(event_tx.clone());
    let scale_factor = window.scale_factor();
    let font_texture_handle = std::thread::spawn(move || {
        profiling::register_thread!("Init FontTexture Thread");

        render::generate_font_texture(scale_factor)
    });
    let handle = window.get_raw_event_handle();
    let (width, height) = window.size();

    std::thread::spawn(move || {
        let (instance, adapters) = adapter_handle.join().unwrap();
        let surface = unsafe { instance.create_surface(&handle) };

        let adapter = adapters
            .into_iter()
            .find(|a| a.is_surface_supported(&surface))
            .expect("Failed to find an appropriate adapter");

        let (output, _master, _shell, msg_rx) = pty_handle.join().unwrap();
        let font_texture = font_texture_handle.join().unwrap();

        render::run(
            surface,
            adapter,
            font_texture,
            width,
            height,
            scale_factor,
            event_rx,
            msg_rx,
            output,
        );
    });

    log::info!("Start window");
    window.run();
}

fn run_reader(input: Box<dyn Read + Send>) -> Receiver<Vec<Action>> {
    let (tx, rx) = crossbeam_channel::bounded(512);

    std::thread::spawn(move || {
        profiling::register_thread!("Reader Thread");
        let mut parser = Parser::new();
        let mut reader = BufReader::new(input);
        let mut buf = [0; 8196];

        loop {
            profiling::scope!("Read");
            match reader.read(&mut buf) {
                Ok(0) => {
                    log::info!("pty input ended");
                    return;
                }
                Ok(len) => {
                    profiling::scope!("Parse");
                    let actions = parser.parse_as_vec(&buf[..len]);
                    tx.send(actions).unwrap();
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(err) => {
                    log::error!("IO error: {}", err);
                    return;
                }
            }
        }
    });

    rx
}
