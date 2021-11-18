use termwiz::escape::{
    csi::{
        Cursor, DecPrivateMode, DecPrivateModeCode, Edit, EraseInDisplay, EraseInLine, Mode,
        TerminalMode, TerminalModeCode,
    },
    Action, ControlCode, CSI,
};

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    ch: char,
}

impl Cell {
    pub fn new(ch: char) -> Self {
        Self { ch }
    }
}

#[derive(Clone)]
pub struct Line {
    raw: Vec<Cell>,
}

impl Line {
    pub fn new(col: usize) -> Self {
        Self {
            raw: vec![Cell::new(' '); col],
        }
    }

    pub fn write_text(&self, out: &mut String) {
        for cell in self.raw.iter() {
            out.push(cell.ch);
        }
    }
}

#[derive(Clone)]
pub struct Terminal {
    grid: Vec<Line>,
    cursor: (usize, usize),
    column: usize,
}

impl Terminal {
    pub fn new(column: usize) -> Self {
        Self {
            grid: vec![Line::new(column)],
            cursor: (0, 0),
            column,
        }
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    pub fn cursor_pos(&self) -> usize {
        self.cursor.0 + self.cursor.1 * self.column
    }

    pub fn rows<'a>(&'a self) -> impl Iterator<Item = &'a Line> + ExactSizeIterator + 'a {
        self.grid.iter()
    }

    pub fn new_row(&mut self) {
        self.grid.push(Line::new(self.column));
    }

    pub fn bs(&mut self) {
        self.cursor.0 = self.cursor.0.saturating_sub(1);
    }

    pub fn lf(&mut self) {
        let new_y = self.cursor.1 + 1;

        if self.grid.len() == new_y {
            self.new_row();
        }

        self.cursor.1 = new_y;
    }

    pub fn cr(&mut self) {
        self.cursor.0 = 0;
    }

    pub fn current_cell_mut(&mut self) -> &mut Cell {
        &mut self.grid[self.cursor.1].raw[self.cursor.0]
    }

    pub fn advance_cursor(&mut self) {
        if self.cursor.0 == self.column - 1 {
            self.cr();
            self.lf();
        } else {
            self.cursor.0 += 1;
        }
    }

    pub fn cursor_up(&mut self, n: usize) {
        self.cursor.1 = self.cursor.1.saturating_sub(n);
    }

    pub fn cursor_down(&mut self, n: usize) {
        for _ in 0..n {
            self.lf();
        }
    }

    pub fn cursor_right(&mut self, n: usize) {
        self.cursor.0 = (self.cursor.0 + n).min(self.column - 1);
    }

    pub fn cursor_left(&mut self, n: usize) {
        self.cursor.0 = self.cursor.0.saturating_sub(n);
    }

    pub fn text(&mut self, c: char) {
        let cell = self.current_cell_mut();
        cell.ch = c;
        self.advance_cursor();
    }

    pub fn erase_all(&mut self) {
        self.grid.clear();
        self.new_row();
        self.cursor = (0, 0);
    }
}

impl Terminal {
    pub fn perform_action(&mut self, action: Action) {
        if !matches!(action, Action::Print(_)) {
            log::debug!("Perform: {:?}", action);
        }

        match action {
            Action::Print(c) => {
                self.text(c);
            }
            Action::Control(ControlCode::Backspace) => self.bs(),
            Action::Control(ControlCode::LineFeed) => self.lf(),
            Action::Control(ControlCode::CarriageReturn) => self.cr(),
            // TODO: style
            Action::CSI(CSI::Edit(Edit::EraseInDisplay(EraseInDisplay::EraseToEndOfDisplay))) => {
                self.erase_all();
            }
            Action::CSI(CSI::Edit(Edit::EraseInDisplay(EraseInDisplay::EraseDisplay))) => {
                self.erase_all();
            }
            Action::CSI(CSI::Edit(Edit::EraseInLine(EraseInLine::EraseToEndOfLine))) => {
                log::info!("Erase line {}", self.cursor.1);
                for cell in self.grid[self.cursor.1].raw[self.cursor.0..].iter_mut() {
                    cell.ch = ' ';
                }
            }
            Action::CSI(CSI::Mode(Mode::ResetMode(TerminalMode::Code(
                TerminalModeCode::ShowCursor,
            )))) => {
                // MS show cursor
            }
            Action::CSI(CSI::Mode(Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::ShowCursor,
            )))) => {
                // show cursor
            }
            Action::CSI(CSI::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::ShowCursor,
            )))) => {
                // hide cursor
            }
            Action::CSI(CSI::Sgr(_)) => {}
            Action::CSI(CSI::Cursor(Cursor::Position { col, line })) => {
                let line = line.as_zero_based() as usize;
                if let Some(new_line) = line.checked_sub(self.grid.len() - 1) {
                    for _ in 0..new_line {
                        self.new_row();
                    }
                }
                self.cursor.0 = (col.as_zero_based() as usize).min(self.column - 1);
                self.cursor.1 = line;
            }
            Action::CSI(CSI::Cursor(Cursor::Up(u))) => {
                self.cursor_up(u as usize);
            }
            Action::CSI(CSI::Cursor(Cursor::Down(d))) => {
                self.cursor_down(d as usize);
            }
            Action::CSI(CSI::Cursor(Cursor::Right(r))) => {
                self.cursor_right(r as usize);
            }
            Action::CSI(CSI::Cursor(Cursor::Left(l))) => {
                self.cursor_left(l as usize);
            }
            other => {
                log::warn!("Unimplemented action {:?}", other);
            }
        }
    }
}

#[test]
fn grid() {
    use termwiz::escape::OneBased;
    let mut grid = Terminal::new(10);
    grid.perform_action(Action::CSI(CSI::Cursor(Cursor::Position {
        col: OneBased::from_zero_based(0),
        line: OneBased::from_zero_based(0),
    })));
    assert_eq!(grid.cursor, (0, 0));
    assert_eq!(grid.cursor_pos(), 0);
    grid.perform_action(Action::CSI(CSI::Cursor(Cursor::Position {
        col: OneBased::from_zero_based(0),
        line: OneBased::from_zero_based(5),
    })));
    assert_eq!(grid.grid.len(), 6);
    assert_eq!(grid.cursor, (0, 5));
    assert_eq!(grid.cursor_pos(), 5 * grid.column);
}
