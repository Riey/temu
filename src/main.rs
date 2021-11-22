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

    let pty_handle = std::thread::spawn(|| {
        profiling::register_thread!("Pty Init Thread");
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
    let (width, height) = window.size();
    let scale_factor = window.scale_factor();

    let instance = wgpu::Instance::new(wgpu::Backends::all());
    let surface = unsafe { instance.create_surface(&window) };

    std::thread::spawn(move || {
        let (output, _master, _shell, msg_rx) = pty_handle.join().unwrap();
        render::run(
            instance,
            surface,
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
