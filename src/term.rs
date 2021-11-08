mod grid;

use parking_lot::Mutex;
use portable_pty::{
    native_pty_system, Child, CommandBuilder, MasterPty, PtySize, PtySystem, SlavePty,
};
use std::{
    env,
    fs::File,
    io::{self, Read, Write},
    os::unix::prelude::FromRawFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use termwiz::escape::parser::Parser;

use crossbeam_channel::{Receiver, Sender};

use crate::event::{TemuEvent, TemuPtyEvent};

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
        for ev in pty_event_rx {
            match ev {
                TemuPtyEvent::Enter => {
                    output.write_all(b"\r").unwrap();
                }
                TemuPtyEvent::Text(t) => {
                    log::debug!("Write: {}", t);
                    output.write_all(t.as_bytes()).unwrap();
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

    let mut pair = pty
        .openpty(PtySize {
            cols: 100,
            rows: 60,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let shell = env::var("SHELL").unwrap();
    let cmd = CommandBuilder::new(shell);
    let child = pair.slave.spawn_command(cmd).unwrap();

    (pair.master, child)
}
