pub enum TemuEvent {
    Resize { width: u32, height: u32 },
    Redraw,
    Close,
    ScrollUp,
    ScrollDown,
}

pub enum TemuPtyEvent {
    Enter,
    Text(String),
}
