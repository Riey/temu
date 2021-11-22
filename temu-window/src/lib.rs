mod event;
mod platform;

pub use self::event::TemuEvent;
pub use crossbeam_channel;

use crossbeam_channel::Sender;

pub trait TemuWindow: raw_window_handle::HasRawWindowHandle {
    fn init(event_tx: Sender<event::TemuEvent>) -> Self;
    fn size(&self) -> (u32, u32);
    fn scale_factor(&self) -> f32;
    fn run(self);
}

#[profiling::function]
pub fn init_native_window(event_tx: Sender<event::TemuEvent>) -> impl TemuWindow {
    self::platform::NativeWindow::init(event_tx)
}
