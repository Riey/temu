mod grid;

use parking_lot::Mutex;
use std::{
    env,
    fs::File,
    io::{self, Read},
    os::unix::prelude::FromRawFd,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crossbeam_channel::Sender;
use nix::pty::{openpty, Winsize};

use crate::event::TemuEvent;

pub use self::grid::{Cell, Grid};

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

#[derive(Clone)]
pub struct Terminal {
    grid: Grid,
}

impl Terminal {
    pub fn new() -> Self {
        Self {
            grid: Grid::new(80),
        }
    }

    pub fn grid(&self) -> &Grid {
        &self.grid
    }
}

pub fn run(_event_tx: Sender<TemuEvent>, shared_terminal: Arc<SharedTerminal>) {
    let mut master_file = start_pty();

    log::info!("pty started");

    let mut need_update = true;
    let mut parser = vte::Parser::new();
    let mut terminal = Terminal::new();
    let mut buffer = [0; 65536];

    loop {
        if need_update {
            need_update = shared_terminal.try_update_terminal(&terminal);
        }
        match master_file.read(&mut buffer) {
            Ok(0) => break,
            Ok(len) => {
                log::debug!("Read {} bytes from pty", len);
                let bytes = &buffer[..len];
                for b in bytes.iter() {
                    parser.advance(&mut terminal.grid, *b);
                }
                // TODO: check update
                need_update = true;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                log::error!("Term Err: {}", e);
                break;
            }
        }
    }
}

fn start_pty() -> File {
    match openpty(
        Some(&Winsize {
            ws_col: 80,
            ws_row: 30,
            ws_xpixel: 1000,
            ws_ypixel: 600,
        }),
        None,
    ) {
        Ok(ret) => {
            let master = unsafe { File::from_raw_fd(ret.master) };
            let shell = env::var("SHELL").unwrap();
            let mut cmd = Command::new(shell);
            unsafe {
                cmd.stdin(Stdio::from_raw_fd(ret.slave));
                cmd.stderr(Stdio::from_raw_fd(ret.slave));
                cmd.stdout(Stdio::from_raw_fd(ret.slave));
            }
            cmd.spawn().unwrap();
            master
        }
        Err(_) => todo!(),
    }
}
