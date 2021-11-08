pub fn channel() -> (Tx, Rx) {
    crossbeam_channel::bounded(100)
}
pub type Rx = crossbeam_channel::Receiver<TemuEvent>;
pub type Tx = crossbeam_channel::Sender<TemuEvent>;

#[derive(Clone, Copy, Debug)]
pub enum TemuEvent {
    Resize { width: u32, height: u32 },
    Redraw,
    Close,
    ScrollUp,
    ScrollDown,
    DpiChange { dpi: i32 },
}
