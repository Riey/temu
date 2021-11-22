use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use termwiz::color::RgbColor;
use wezterm_term::{color::ColorPalette, TerminalConfiguration};

#[derive(Clone, Debug)]
pub struct TerminalConfig;

impl TerminalConfiguration for TerminalConfig {
    fn color_palette(&self) -> ColorPalette {
        ColorPalette {
            background: RgbColor::new_f32(
                crate::DEFAULT_BG[0],
                crate::DEFAULT_BG[1],
                crate::DEFAULT_BG[2],
            ),
            foreground: RgbColor::new_f32(
                crate::DEFAULT_TEXT[0],
                crate::DEFAULT_TEXT[1],
                crate::DEFAULT_TEXT[2],
            ),
            ..Default::default()
        }
    }
}

pub fn start_pty() -> (Box<dyn MasterPty + Send>, Box<dyn Child + Send + Sync>) {
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
