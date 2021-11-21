pub enum TemuEvent {
    Resize { width: u32, height: u32 },
    CursorMove { x: f32, y: f32 },
    Left(bool),

    Redraw,
    Close,
    ScrollUp,
    ScrollDown,
    Char(char),
}
