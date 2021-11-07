use parking_lot::{Mutex, MutexGuard};
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

pub struct SharedTerminal {
    terminal: Mutex<Terminal>,
    changed: AtomicBool,
}

impl SharedTerminal {
    pub fn new() -> Self {
        Self {
            terminal: Mutex::new(Terminal {}),
            changed: AtomicBool::new(false),
        }
    }

    pub fn take_terminal(&self) -> Option<MutexGuard<'_, Terminal>> {
        if self.changed.swap(false, Ordering::Acquire) {
            Some(self.terminal.lock())
        } else {
            None
        }
    }

    pub fn try_update_terminal(&self, terminal: &Terminal) -> bool {
        if let Some(mut lock) = self.terminal.try_lock() {
            *lock = terminal.clone();
            self.changed.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }
}

#[derive(Clone)]
pub struct Terminal {}

pub fn run(_event_tx: Sender<TemuEvent>, shared_terminal: Arc<SharedTerminal>) {
    let mut master_file = start_pty();

    let mut need_update = true;
    let mut parser = vte::Parser::new();
    let mut terminal = Terminal {};
    let mut buffer = [0; 65536];

    eprintln!("Start term");

    loop {
        if need_update {
            need_update = shared_terminal.try_update_terminal(&terminal);
        }
        match master_file.read(&mut buffer) {
            Ok(0) => break,
            Ok(len) => {
                eprintln!("Read {} bytes from pty", len);
                let bytes = &buffer[..len];
                for b in bytes.iter() {
                    parser.advance(&mut terminal, *b);
                }
                // TODO: check update
                need_update = true;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("Term Err: {}", e);
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

impl vte::Perform for Terminal {
    fn print(&mut self, c: char) {
        print!("{}", c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            10 => {}
            _ => {}
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }

    fn put(&mut self, byte: u8) {
        println!("put: {}", byte);
    }

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn csi_dispatch(
        &mut self,
        _params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}
