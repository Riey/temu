use std::iter;
use termwiz::escape::{csi::Cursor, Action, ControlCode, Esc, EscCode, CSI};

#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
}

#[derive(Clone)]
pub struct Grid {
    rows: Vec<Cell>,
    column: usize,
    row: usize,
    cursor: (usize, usize),
}

fn empty_row(column: usize) -> impl Iterator<Item = Cell> {
    let row_iter = iter::repeat(Cell { ch: ' ' }).take(column);

    row_iter
}

impl Grid {
    pub fn new(column: usize) -> Self {
        Self {
            rows: empty_row(column).collect(),
            row: 1,
            column,
            cursor: (0, 0),
        }
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    pub fn rows(&self) -> impl Iterator<Item = &[Cell]> + '_ {
        self.rows.chunks(self.column)
    }

    fn get_slice(&self, x: usize, y: usize) -> Option<&[Cell]> {
        self.rows.get(y * self.column + x..)
    }

    fn get_mut_slice(&mut self, x: usize, y: usize) -> Option<&mut [Cell]> {
        self.rows.get_mut(y * self.column + x..)
    }

    pub fn lf(&mut self) {
        let new_y = self.cursor.1 + 1;

        if self.row == new_y {
            self.rows.extend(empty_row(self.column));
            self.row += 1;
        }

        self.cursor.1 = new_y;
    }

    pub fn cr(&mut self) {
        self.cursor.0 = 0;
    }

    pub fn current_cell_mut(&mut self) -> &mut Cell {
        self.rows
            .get_mut(self.cursor.1 * self.column + self.cursor.0)
            .unwrap()
    }

    pub fn advance_cursor(&mut self) {
        if self.cursor.0 as usize == self.column - 1 {
            self.lf();
        } else {
            self.cursor.0 += 1;
        }
    }

    pub fn cursor_right(&mut self, n: usize) {
        self.cursor.0 = (self.cursor.0 + n).max(self.column - 1);
    }

    pub fn cursor_left(&mut self, n: usize) {
        self.cursor.0 = self.cursor.0.saturating_sub(n);
    }

    pub fn text(&mut self, c: char) {
        let cell = self.current_cell_mut();
        cell.ch = c;
        self.advance_cursor();
    }
}

impl Grid {
    pub fn perform_action(&mut self, action: Action) {
        match action {
            Action::Print(c) => {
                self.text(c);
            }
            Action::Control(ControlCode::LineFeed) => self.lf(),
            Action::Control(ControlCode::CarriageReturn) => self.cr(),
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
    let mut grid = Grid::new(10);
    assert_eq!(grid.rows().count(), grid.row);
    grid.lf();
    assert_eq!(grid.rows().count(), grid.row);
}
