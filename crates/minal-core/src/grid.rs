//! Terminal grid: rows of cells forming the visible screen.

use crate::cell::Cell;

/// A single row of terminal cells.
#[derive(Debug, Clone)]
pub struct Row {
    cells: Vec<Cell>,
}

impl Row {
    /// Create a new row with the given number of columns.
    pub fn new(cols: usize) -> Self {
        Self {
            cells: vec![Cell::default(); cols],
        }
    }

    /// Number of columns in this row.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether the row is empty (zero columns).
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Access a cell by column index.
    pub fn get(&self, col: usize) -> Option<&Cell> {
        self.cells.get(col)
    }

    /// Mutably access a cell by column index.
    pub fn get_mut(&mut self, col: usize) -> Option<&mut Cell> {
        self.cells.get_mut(col)
    }

    /// Reset all cells to default.
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
    }
}

/// The terminal grid: a 2D array of cells.
#[derive(Debug, Clone)]
pub struct Grid {
    rows: Vec<Row>,
    cols: usize,
    num_rows: usize,
}

impl Grid {
    /// Create a new grid with the given dimensions.
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows: (0..rows).map(|_| Row::new(cols)).collect(),
            cols,
            num_rows: rows,
        }
    }

    /// Number of rows.
    pub fn rows(&self) -> usize {
        self.num_rows
    }

    /// Number of columns.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Access a row by index.
    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }

    /// Mutably access a row by index.
    pub fn row_mut(&mut self, index: usize) -> Option<&mut Row> {
        self.rows.get_mut(index)
    }

    /// Clear the entire grid.
    pub fn clear(&mut self) {
        for row in &mut self.rows {
            row.clear();
        }
    }

    /// Resize the grid, preserving content where possible.
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        // Adjust columns in existing rows
        for row in &mut self.rows {
            row.cells.resize(new_cols, Cell::default());
        }

        // Add or remove rows
        self.rows.resize_with(new_rows, || Row::new(new_cols));

        self.num_rows = new_rows;
        self.cols = new_cols;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_creation() {
        let grid = Grid::new(24, 80);
        assert_eq!(grid.rows(), 24);
        assert_eq!(grid.cols(), 80);
    }

    #[test]
    fn test_grid_resize() {
        let mut grid = Grid::new(24, 80);
        grid.resize(30, 120);
        assert_eq!(grid.rows(), 30);
        assert_eq!(grid.cols(), 120);
    }

    #[test]
    fn test_row_default_cells() {
        let row = Row::new(80);
        assert_eq!(row.len(), 80);
        assert_eq!(row.get(0).unwrap().c, ' ');
    }
}
