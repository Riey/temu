use std::iter;

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

        self.cursor = (0, new_y);
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
}

impl vte::Perform for Grid {
    fn print(&mut self, c: char) {
        log::trace!("Print: {}", c);
        self.current_cell_mut().ch = c;
        self.advance_cursor();
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.lf(),
            _ => {}
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
