//! Terminal grid: rows of cells forming the visible screen.

use crate::cell::Cell;

/// A single row of terminal cells.
#[derive(Debug, Clone)]
pub struct Row {
    cells: Vec<Cell>,
    /// Whether this row has been modified since last render.
    pub dirty: bool,
}

impl Row {
    /// Create a new row with the given number of columns.
    pub fn new(cols: usize) -> Self {
        Self {
            cells: vec![Cell::default(); cols],
            dirty: true,
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
        self.dirty = true;
        self.cells.get_mut(col)
    }

    /// Access the underlying cells slice.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Resize the row to `new_cols` columns, filling new cells with the default.
    pub fn resize(&mut self, new_cols: usize) {
        self.cells.resize(new_cols, Cell::default());
        self.dirty = true;
    }

    /// Reset all cells to default.
    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
        self.dirty = true;
    }

    /// Clear cells from `start` to end of row.
    pub fn clear_from(&mut self, start: usize) {
        for cell in self.cells.iter_mut().skip(start) {
            cell.reset();
        }
        self.dirty = true;
    }

    /// Clear cells from start of row to `end` (inclusive).
    pub fn clear_to(&mut self, end: usize) {
        for cell in self.cells.iter_mut().take(end + 1) {
            cell.reset();
        }
        self.dirty = true;
    }

    /// Insert `count` blank cells at position, pushing existing cells right.
    /// Cells that exceed the row width are discarded.
    pub fn insert_cells(&mut self, col: usize, count: usize) {
        let len = self.cells.len();
        if col >= len {
            return;
        }
        for _ in 0..count.min(len - col) {
            self.cells.insert(col, Cell::default());
        }
        self.cells.truncate(len);
        self.dirty = true;
    }

    /// Delete `count` cells at position, shifting remaining cells left.
    /// New blank cells are appended at the end.
    pub fn delete_cells(&mut self, col: usize, count: usize) {
        let len = self.cells.len();
        if col >= len {
            return;
        }
        let actual = count.min(len - col);
        self.cells.drain(col..col + actual);
        self.cells.resize(len, Cell::default());
        self.dirty = true;
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

    /// Access a cell at (row, col).
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.rows.get(row).and_then(|r| r.get(col))
    }

    /// Mutably access a cell at (row, col).
    pub fn cell_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
        self.rows.get_mut(row).and_then(|r| r.get_mut(col))
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
            row.resize(new_cols);
        }

        // Add or remove rows
        self.rows.resize_with(new_rows, || Row::new(new_cols));

        self.num_rows = new_rows;
        self.cols = new_cols;
    }

    /// Scroll lines up within a region `[top, bottom)`.
    ///
    /// Lines at the top of the region scroll out and are returned.
    /// New blank lines appear at the bottom of the region.
    pub fn scroll_up(&mut self, top: usize, bottom: usize, count: usize) -> Vec<Row> {
        let bottom = bottom.min(self.num_rows);
        if top >= bottom || count == 0 {
            return Vec::new();
        }

        let actual = count.min(bottom - top);
        let scrolled: Vec<Row> = self.rows[top..top + actual].to_vec();

        // Remove scrolled-out rows
        self.rows.drain(top..top + actual);

        // Insert blank rows at the bottom of the region
        let insert_pos = bottom - actual;
        for i in 0..actual {
            self.rows.insert(insert_pos + i, Row::new(self.cols));
        }

        scrolled
    }

    /// Scroll lines down within a region `[top, bottom)`.
    ///
    /// Lines at the bottom of the region are discarded.
    /// New blank lines appear at the top of the region.
    pub fn scroll_down(&mut self, top: usize, bottom: usize, count: usize) {
        let bottom = bottom.min(self.num_rows);
        if top >= bottom || count == 0 {
            return;
        }

        let actual = count.min(bottom - top);

        // Remove lines from bottom of region
        let drain_start = bottom - actual;
        self.rows.drain(drain_start..bottom);

        // Insert blank lines at top of region
        for i in 0..actual {
            self.rows.insert(top + i, Row::new(self.cols));
        }
    }

    /// Insert `count` blank lines at `row`, scrolling existing lines down
    /// within `[row, bottom)`. Lines that scroll past `bottom` are discarded.
    pub fn insert_lines(&mut self, row: usize, bottom: usize, count: usize) {
        if row >= bottom || row >= self.num_rows {
            return;
        }
        self.scroll_down(row, bottom, count);
    }

    /// Delete `count` lines at `row`, scrolling lines up within `[row, bottom)`.
    /// New blank lines appear at the bottom of the region.
    pub fn delete_lines(&mut self, row: usize, bottom: usize, count: usize) -> Vec<Row> {
        if row >= bottom || row >= self.num_rows {
            return Vec::new();
        }
        self.scroll_up(row, bottom, count)
    }

    /// Mark all rows as clean (not dirty).
    pub fn clear_dirty(&mut self) {
        for row in &mut self.rows {
            row.dirty = false;
        }
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

    #[test]
    fn test_grid_cell_access() {
        let mut grid = Grid::new(24, 80);
        if let Some(cell) = grid.cell_mut(5, 10) {
            cell.c = 'A';
        }
        assert_eq!(grid.cell(5, 10).unwrap().c, 'A');
    }

    #[test]
    fn test_scroll_up() {
        let mut grid = Grid::new(5, 10);
        // Mark first row
        if let Some(cell) = grid.cell_mut(0, 0) {
            cell.c = 'X';
        }

        let scrolled = grid.scroll_up(0, 5, 1);
        assert_eq!(scrolled.len(), 1);
        assert_eq!(scrolled[0].get(0).unwrap().c, 'X');
        // First row should now be what was the second row
        assert_eq!(grid.cell(0, 0).unwrap().c, ' ');
        // Last row should be blank
        assert_eq!(grid.cell(4, 0).unwrap().c, ' ');
    }

    #[test]
    fn test_scroll_down() {
        let mut grid = Grid::new(5, 10);
        // Mark last row
        if let Some(cell) = grid.cell_mut(4, 0) {
            cell.c = 'Y';
        }

        grid.scroll_down(0, 5, 1);
        // First row should be blank
        assert_eq!(grid.cell(0, 0).unwrap().c, ' ');
        // The row that was at 4 (marked 'Y') is discarded, row at 3 moved to 4
        assert_eq!(grid.cell(4, 0).unwrap().c, ' ');
    }

    #[test]
    fn test_scroll_region() {
        let mut grid = Grid::new(5, 10);
        for i in 0..5 {
            if let Some(cell) = grid.cell_mut(i, 0) {
                cell.c = char::from(b'A' + i as u8);
            }
        }

        // Scroll rows 1..4 up by 1
        let scrolled = grid.scroll_up(1, 4, 1);
        assert_eq!(scrolled.len(), 1);
        assert_eq!(scrolled[0].get(0).unwrap().c, 'B');
        // Row 0 unchanged
        assert_eq!(grid.cell(0, 0).unwrap().c, 'A');
        // Row 1 now has what was row 2
        assert_eq!(grid.cell(1, 0).unwrap().c, 'C');
        // Row 2 now has what was row 3
        assert_eq!(grid.cell(2, 0).unwrap().c, 'D');
        // Row 3 is new blank
        assert_eq!(grid.cell(3, 0).unwrap().c, ' ');
        // Row 4 unchanged
        assert_eq!(grid.cell(4, 0).unwrap().c, 'E');
    }

    #[test]
    fn test_row_insert_cells() {
        let mut row = Row::new(5);
        if let Some(cell) = row.get_mut(0) {
            cell.c = 'A';
        }
        if let Some(cell) = row.get_mut(1) {
            cell.c = 'B';
        }
        row.insert_cells(1, 1);
        assert_eq!(row.len(), 5);
        assert_eq!(row.get(0).unwrap().c, 'A');
        assert_eq!(row.get(1).unwrap().c, ' '); // inserted blank
        assert_eq!(row.get(2).unwrap().c, 'B'); // shifted
    }

    #[test]
    fn test_row_delete_cells() {
        let mut row = Row::new(5);
        if let Some(cell) = row.get_mut(0) {
            cell.c = 'A';
        }
        if let Some(cell) = row.get_mut(1) {
            cell.c = 'B';
        }
        if let Some(cell) = row.get_mut(2) {
            cell.c = 'C';
        }
        row.delete_cells(1, 1);
        assert_eq!(row.len(), 5);
        assert_eq!(row.get(0).unwrap().c, 'A');
        assert_eq!(row.get(1).unwrap().c, 'C');
        assert_eq!(row.get(4).unwrap().c, ' '); // filled blank
    }

    #[test]
    fn test_row_clear_from() {
        let mut row = Row::new(5);
        for i in 0..5 {
            if let Some(cell) = row.get_mut(i) {
                cell.c = char::from(b'A' + i as u8);
            }
        }
        row.clear_from(3);
        assert_eq!(row.get(2).unwrap().c, 'C');
        assert_eq!(row.get(3).unwrap().c, ' ');
        assert_eq!(row.get(4).unwrap().c, ' ');
    }

    #[test]
    fn test_row_clear_to() {
        let mut row = Row::new(5);
        for i in 0..5 {
            if let Some(cell) = row.get_mut(i) {
                cell.c = char::from(b'A' + i as u8);
            }
        }
        row.clear_to(1);
        assert_eq!(row.get(0).unwrap().c, ' ');
        assert_eq!(row.get(1).unwrap().c, ' ');
        assert_eq!(row.get(2).unwrap().c, 'C');
    }

    #[test]
    fn test_insert_lines() {
        let mut grid = Grid::new(5, 10);
        for i in 0..5 {
            if let Some(cell) = grid.cell_mut(i, 0) {
                cell.c = char::from(b'A' + i as u8);
            }
        }
        grid.insert_lines(2, 5, 1);
        assert_eq!(grid.cell(0, 0).unwrap().c, 'A');
        assert_eq!(grid.cell(1, 0).unwrap().c, 'B');
        assert_eq!(grid.cell(2, 0).unwrap().c, ' '); // inserted blank
        assert_eq!(grid.cell(3, 0).unwrap().c, 'C');
        assert_eq!(grid.cell(4, 0).unwrap().c, 'D');
        // 'E' was pushed out
    }

    #[test]
    fn test_delete_lines() {
        let mut grid = Grid::new(5, 10);
        for i in 0..5 {
            if let Some(cell) = grid.cell_mut(i, 0) {
                cell.c = char::from(b'A' + i as u8);
            }
        }
        let deleted = grid.delete_lines(1, 5, 1);
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].get(0).unwrap().c, 'B');
        assert_eq!(grid.cell(0, 0).unwrap().c, 'A');
        assert_eq!(grid.cell(1, 0).unwrap().c, 'C');
        assert_eq!(grid.cell(2, 0).unwrap().c, 'D');
        assert_eq!(grid.cell(3, 0).unwrap().c, 'E');
        assert_eq!(grid.cell(4, 0).unwrap().c, ' '); // new blank
    }
}
