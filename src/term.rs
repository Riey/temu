use std::{env, os::unix::prelude::RawFd, process::Command};

use crossbeam_channel::Sender;
use nix::{
    errno::Errno,
    pty::{forkpty, Winsize},
    unistd::{read, ForkResult},
};

use crate::event::TemuEvent;

pub fn run(event_tx: Sender<TemuEvent>, stdout_fd: RawFd) {
    let mut buffer = [0; 65536];

    eprintln!("Start term");

    loop {
        match read(stdout_fd, &mut buffer) {
            Ok(0) => break,
            Ok(len) => {
                let bytes = &buffer[..len];
                eprintln!("Read: {}", String::from_utf8_lossy(bytes));
            }
            Err(e) if e == Errno::EINTR => continue,
            Err(e) => {
                eprintln!("Term Err: {}", e);
                break;
            }
        }
    }
}

pub fn get_shell_stdout_fd() -> RawFd {
    let shell = env::var("SHELL").expect("No $SHELL");

    match unsafe {
        forkpty(
            Some(&Winsize {
                ws_col: 80,
                ws_row: 30,
                ws_xpixel: 1000,
                ws_ypixel: 600,
            }),
            None,
        )
    } {
        Ok(forkpty_ret) => {
            if let ForkResult::Child = forkpty_ret.fork_result {
                Command::new(&shell).spawn().unwrap();
                eprintln!("Closed");
                std::thread::sleep(std::time::Duration::from_millis(2000));
                std::process::exit(0);
            }
            let stdout_fd = forkpty_ret.master;
            stdout_fd
        }
        Err(_) => todo!(),
    }
}
