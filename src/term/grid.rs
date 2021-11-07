use std::{borrow::Cow, iter, sync::Arc};

#[derive(Clone)]
pub enum Cell {
    /// Start position
    Start(String),
    /// It has same attributes + color + bg_color and get next char position of left cell
    Merged,
}

#[derive(Clone)]
pub struct Grid {
    rows: Vec<Vec<Cell>>,
    column: usize,
    latest_start_cell: usize,
    cursor: (usize, usize),
}

fn empty_row(column: usize) -> Vec<Cell> {
    let row_iter = iter::once(Cell::Start(" ".repeat(column).into()))
        .chain(iter::repeat(Cell::Merged))
        .take(column);

    row_iter.collect()
}

impl Grid {
    pub fn new(column: usize) -> Self {
        Self {
            rows: vec![empty_row(column); 1],
            column,
            latest_start_cell: 0,
            cursor: (0, 0),
        }
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    pub fn rows(&self) -> impl Iterator<Item = &[Cell]> + '_ {
        self.rows.iter().map(Vec::as_slice)
    }

    pub fn lf(&mut self) {
        let new_y = self.cursor.1 + 1;

        if let Some(next_row) = self.rows.get(new_y as usize) {
            self.latest_start_cell = next_row
                .iter()
                .position(|c| matches!(c, Cell::Merged))
                .unwrap();
        } else {
            self.rows.reserve(10);
            self.rows.push(empty_row(self.column));
            self.latest_start_cell = 0;
        }

        self.cursor = (0, new_y);
    }

    fn current_start_cell_mut(&mut self) -> &mut Cell {
        &mut self.rows[self.cursor.1][self.latest_start_cell]
    }

    fn current_cell_mut(&mut self) -> &mut Cell {
        &mut self.rows[self.cursor.1][self.cursor.0]
    }
}

impl vte::Perform for Grid {
    fn print(&mut self, c: char) {
        *self.current_cell_mut() = Cell::Merged;
        match self.current_start_cell_mut() {
            Cell::Start(s) => s.push(c),
            Cell::Merged => unreachable!(),
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.lf(),
            _ => {}
        }
    }
}
