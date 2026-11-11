//! The matrix writer: a grid of cells to type numbers into, which becomes
//! a matrix (or a vector, when it is one row) on the stack.

use ratatui::crossterm::event::KeyCode;

pub struct MatrixWriter {
    pub cells: Vec<Vec<String>>,
    /// The cursor. A column past the last, or a row past the last, is where
    /// a new one goes once something is entered there.
    pub row: usize,
    pub col: usize,
    /// What is being typed into the cell under the cursor.
    pub edit: Option<String>,
    /// Opened by VIEW on level 1: the result replaces it.
    pub replace: bool,
}

/// What a key in the matrix writer leaves to do.
pub enum Done {
    Not,
    Cancel,
    /// The array's source, to enter.
    Enter(String),
}

impl MatrixWriter {
    pub fn new(cells: Vec<Vec<String>>, replace: bool) -> Self {
        MatrixWriter {
            cells,
            row: 0,
            col: 0,
            edit: None,
            replace,
        }
    }

    pub fn width(&self) -> usize {
        self.cells.first().map_or(0, Vec::len)
    }

    /// Puts a value in the cell under the cursor, growing the grid by a
    /// column or row of zeros when the cursor is past its edge.
    fn set(&mut self, v: String) {
        if self.col == self.width() {
            if self.cells.is_empty() {
                self.cells.push(vec![]);
            }
            for r in &mut self.cells {
                r.push("0".into());
            }
        }
        if self.row == self.cells.len() {
            self.cells.push(vec!["0".into(); self.width()]);
        }
        self.cells[self.row][self.col] = v;
    }

    /// After entering a cell: along the first row while it is the only one,
    /// so that it sets the width, and then row by row.
    fn advance(&mut self) {
        if self.cells.len() == 1 || self.col + 1 < self.width() {
            self.col += 1;
        } else {
            (self.row, self.col) = (self.row + 1, 0);
        }
    }

    fn commit(&mut self) -> bool {
        match self.edit.take() {
            Some(s) if !s.trim().is_empty() => {
                self.set(s.trim().to_string());
                self.advance();
                true
            }
            _ => false,
        }
    }

    /// The source of the array: [ … ] for one row, [[ … ] …] for more.
    pub fn source(&self) -> Option<String> {
        let row = |r: &Vec<String>| format!("[ {} ]", r.join(" "));
        match &self.cells[..] {
            [] => None,
            [r] if r.is_empty() => None,
            [r] => Some(row(r)),
            rows => Some(format!(
                "[{}]",
                rows.iter().map(row).collect::<Vec<_>>().join(" ")
            )),
        }
    }

    pub fn key(&mut self, code: KeyCode) -> Done {
        let (rows, w) = (self.cells.len(), self.width());
        match code {
            KeyCode::Char(c) => self.edit.get_or_insert_default().push(c),
            KeyCode::Backspace => {
                if let Some(e) = &mut self.edit {
                    e.pop();
                }
            }
            KeyCode::Enter => {
                if !self.commit() && self.edit.take().is_none() {
                    return match self.source() {
                        Some(s) => Done::Enter(s),
                        None => Done::Cancel,
                    };
                }
            }
            KeyCode::Esc if self.edit.is_some() => self.edit = None,
            KeyCode::Esc => return Done::Cancel,
            // Delete removes the cursor's row, or its column on one row.
            KeyCode::Delete if self.row < rows && self.col < w => {
                if rows > 1 {
                    self.cells.remove(self.row);
                } else {
                    self.cells[0].remove(self.col);
                }
                self.row = self.row.min(self.cells.len().saturating_sub(1));
                self.col = self.col.min(self.width());
            }
            _ => {
                // The arrows enter what is typed first, as the calculator's do.
                if self.edit.is_some() {
                    let s = self.edit.take().unwrap_or_default();
                    if !s.trim().is_empty() {
                        self.set(s.trim().to_string());
                    }
                }
                let (rows, w) = (self.cells.len(), self.width());
                match code {
                    KeyCode::Left => self.col = self.col.saturating_sub(1),
                    KeyCode::Right => self.col = (self.col + 1).min(w),
                    KeyCode::Up => self.row = self.row.saturating_sub(1),
                    KeyCode::Down => self.row = (self.row + 1).min(rows),
                    KeyCode::Home => self.col = 0,
                    KeyCode::End => self.col = w.saturating_sub(1),
                    _ => {}
                }
                // Past the edge only one way at a time: a new row starts at
                // its first column.
                if self.row == rows && self.col == w {
                    self.col = 0;
                }
            }
        }
        Done::Not
    }
}

#[cfg(test)]
mod tests {
    use super::{Done, MatrixWriter};
    use ratatui::crossterm::event::KeyCode;

    fn typed(m: &mut MatrixWriter, keys: &str) -> Option<String> {
        let mut out = None;
        for c in keys.chars() {
            let code = match c {
                '\n' => KeyCode::Enter,
                'v' => KeyCode::Down,
                c => KeyCode::Char(c),
            };
            if let Done::Enter(s) = m.key(code) {
                out = Some(s);
            }
        }
        out
    }

    #[test]
    fn matrix_writer() {
        let mut m = MatrixWriter::new(vec![], false);
        assert_eq!(typed(&mut m, "1\n2\n3\n\n").as_deref(), Some("[ 1 2 3 ]"));
        let mut m = MatrixWriter::new(vec![], false);
        let s = typed(&mut m, "1\n2\nv3\n4\n\n");
        assert_eq!(s.as_deref(), Some("[[ 1 2 ] [ 3 4 ]]"));
        let mut m = MatrixWriter::new(vec![vec!["5".into(), "6".into()]], true);
        assert_eq!(typed(&mut m, "7\n\n").as_deref(), Some("[ 7 6 ]"));
    }
}
