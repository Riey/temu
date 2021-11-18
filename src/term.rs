mod grid;

use crossbeam_utils::atomic::AtomicCell;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::{io::{self, BufReader, Read, Write}, sync::Arc};
use termwiz::escape::parser::Parser;

use crossbeam_channel::{Receiver, Sender};

use temu_window::{TemuEvent, TemuPtyEvent};

pub use self::grid::{Cell, Terminal};

pub struct SharedTerminal {
    terminal: AtomicCell<Option<Terminal>>,
}

impl SharedTerminal {
    pub fn new() -> Self {
        Self {
            terminal: AtomicCell::new(None),
        }
    }

    pub fn take_terminal(&self) -> Option<Terminal> {
        self.terminal.take()
    }

    pub fn update_terminal(&self, terminal: &Terminal) {
        self.terminal.store(Some(terminal.clone()));
    }
}

pub fn run(
    _event_tx: Sender<TemuEvent>,
    pty_event_rx: Receiver<TemuPtyEvent>,
    shared_terminal: Arc<SharedTerminal>,
) {
    let (master, _shell) = start_pty();

    let mut input = BufReader::new(master.try_clone_reader().unwrap());

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
    let mut grid = Terminal::new(crate::COLUMN as _);
    let mut parser = Parser::new();

    loop {
        if need_update {
            shared_terminal.update_terminal(&grid);
            need_update = false;
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
            cols: crate::COLUMN as _,
            rows: crate::ROW as _,
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
