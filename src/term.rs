mod grid;

use parking_lot::Mutex;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::{
    io::{self, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use termwiz::escape::parser::Parser;

use crossbeam_channel::{Receiver, Sender};

use temu_window::{TemuEvent, TemuPtyEvent};

pub use self::grid::{Cell, Terminal};

pub struct SharedTerminal {
    terminal: Mutex<Option<Terminal>>,
    changed: AtomicBool,
}

impl SharedTerminal {
    pub fn new() -> Self {
        Self {
            terminal: Mutex::new(None),
            changed: AtomicBool::new(false),
        }
    }

    pub fn take_terminal(&self) -> Option<Terminal> {
        if self.changed.swap(false, Ordering::Acquire) {
            self.terminal.lock().take()
        } else {
            None
        }
    }

    pub fn try_update_terminal(&self, terminal: &Terminal) -> bool {
        if let Some(mut lock) = self.terminal.try_lock() {
            *lock = Some(terminal.clone());
            self.changed.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }
}

pub fn run(
    _event_tx: Sender<TemuEvent>,
    pty_event_rx: Receiver<TemuPtyEvent>,
    shared_terminal: Arc<SharedTerminal>,
) {
    let (master, _shell) = start_pty();

    let mut input = master.try_clone_reader().unwrap();

    let mut output = master.try_clone_writer().unwrap();

    std::thread::spawn(move || {
        let mut buf = [0u8; 8];
        for ev in pty_event_rx {
            match ev {
                TemuPtyEvent::Char(c) => {
                    output
                        .write_all(c.encode_utf8(&mut buf).as_bytes())
                        .unwrap();
                }
            }
        }
    });

    log::info!("pty started");

    let mut need_update = false;
    let mut buffer = [0; 65536];
    let mut grid = Terminal::new(100);
    let mut parser = Parser::new();

    loop {
        if need_update {
            need_update = shared_terminal.try_update_terminal(&grid);
        }
        match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(len) => {
                log::debug!("Read {} bytes from pty", len);
                let bytes = &buffer[..len];
                parser.parse(bytes, |action| {
                    grid.perform_action(action);
                });
                need_update = true;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                log::error!("Term Err: {}", e);
                break;
            }
        }
    }

    log::error!("pty ended");
}

fn start_pty() -> (Box<dyn MasterPty + Send>, Box<dyn Child + Send + Sync>) {
    let pty = native_pty_system();

    let pair = pty
        .openpty(PtySize {
            cols: 60,
            rows: 20,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    #[cfg(unix)]
    let shell = std::env::var("SHELL").unwrap();
    #[cfg(windows)]
    let shell = "powershell";
    let cmd = CommandBuilder::new(shell);
    let child = pair.slave.spawn_command(cmd).unwrap();

    (pair.master, child)
}
