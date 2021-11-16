mod event;
mod platform;

pub use self::event::{TemuEvent, TemuPtyEvent};
pub use crossbeam_channel;

use crossbeam_channel::Sender;

pub trait TemuWindow: raw_window_handle::HasRawWindowHandle {
    fn init(event_tx: Sender<event::TemuEvent>, pty_event_tx: Sender<TemuPtyEvent>) -> Self;
    fn size(&self) -> (u32, u32);
    fn scale_factor(&self) -> f32;
    fn run(self);
}

pub fn init_native_window(
    event_tx: Sender<event::TemuEvent>,
    pty_event_tx: Sender<TemuPtyEvent>,
) -> impl TemuWindow {
    self::platform::NativeWindow::init(event_tx, pty_event_tx)
}
