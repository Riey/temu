use grid::Grid;
use std::iter;
use termwiz::escape::{
    csi::{Cursor, Edit, EraseInDisplay},
    Action, ControlCode, Esc, EscCode, CSI,
};

#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
}

impl Default for Cell {
    fn default() -> Self {
        Self { ch: ' ' }
    }
}

#[derive(Clone)]
pub struct Terminal {
    grid: Grid<Cell>,
    cursor: (usize, usize),
}

fn empty_row(column: usize) -> impl Iterator<Item = Cell> {
    let row_iter = iter::repeat(Cell { ch: ' ' }).take(column);

    row_iter
}

impl Terminal {
    pub fn new(column: usize) -> Self {
        Self {
            grid: Grid::new(1, column),
            cursor: (0, 0),
        }
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    pub fn rows<'a>(
        &'a self,
    ) -> impl Iterator<Item = impl Iterator<Item = &'a Cell> + ExactSizeIterator + 'a> {
        (0..self.grid.rows()).map(move |i| self.grid.iter_row(i))
    }

    pub fn lf(&mut self) {
        let new_y = self.cursor.1 + 1;

        if self.grid.rows() == new_y {
            let col = self.grid.cols();
            self.grid.push_row(empty_row(col).collect());
        }

        self.cursor.1 = new_y;
    }

    pub fn cr(&mut self) {
        self.cursor.0 = 0;
    }

    pub fn current_cell_mut(&mut self) -> &mut Cell {
        self.grid
            .get_mut(self.cursor.1, self.cursor.0)
            .expect("Index out of range")
    }

    pub fn advance_cursor(&mut self) {
        if self.cursor.0 as usize == self.grid.cols() - 1 {
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
        self.cursor.0 = (self.cursor.0 + n).max(self.grid.cols() - 1);
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
        let col = self.grid.cols();
        self.grid.clear();
        self.grid.push_row(empty_row(col).collect());
        self.cursor = (0, 0);
    }
}

impl Terminal {
    pub fn perform_action(&mut self, action: Action) {
        match action {
            Action::Print(c) => {
                self.text(c);
            }
            Action::Control(ControlCode::LineFeed) => self.lf(),
            Action::Control(ControlCode::CarriageReturn) => self.cr(),
            // TODO: style
            Action::CSI(CSI::Edit(Edit::EraseInDisplay(EraseInDisplay::EraseToEndOfDisplay))) => {
                self.erase_all();
            }
            Action::CSI(CSI::Sgr(_)) => {}
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
    let mut grid = Terminal::new(10);
    assert_eq!(grid.rows().count(), grid.row);
    grid.lf();
    assert_eq!(grid.rows().count(), grid.row);
}
