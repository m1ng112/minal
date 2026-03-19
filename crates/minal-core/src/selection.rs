//! Text selection framework for the terminal.

/// Selection type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionType {
    /// Normal character-wise selection.
    Simple,
    /// Block (rectangular) selection.
    Block,
    /// Line-wise selection.
    Lines,
}

/// A point in the terminal grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    /// Row index (can be negative for scrollback).
    pub row: i32,
    /// Column index.
    pub col: usize,
}

impl SelectionPoint {
    /// Create a new selection point.
    pub fn new(row: i32, col: usize) -> Self {
        Self { row, col }
    }
}

impl PartialOrd for SelectionPoint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SelectionPoint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.row.cmp(&other.row).then(self.col.cmp(&other.col))
    }
}

/// A text selection in the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Selection type.
    pub ty: SelectionType,
    /// Starting point of the selection (anchor).
    pub start: SelectionPoint,
    /// Current end point of the selection.
    pub end: SelectionPoint,
}

impl Selection {
    /// Create a new selection starting at the given point.
    pub fn new(ty: SelectionType, point: SelectionPoint) -> Self {
        Self {
            ty,
            start: point,
            end: point,
        }
    }

    /// Update the selection endpoint.
    pub fn update(&mut self, point: SelectionPoint) {
        self.end = point;
    }

    /// Get the normalized selection bounds (start <= end).
    pub fn bounds(&self) -> (SelectionPoint, SelectionPoint) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// Check if a point is within this selection.
    pub fn contains(&self, point: SelectionPoint) -> bool {
        let (start, end) = self.bounds();

        match self.ty {
            SelectionType::Simple => {
                if point.row < start.row || point.row > end.row {
                    return false;
                }
                if point.row == start.row && point.col < start.col {
                    return false;
                }
                if point.row == end.row && point.col > end.col {
                    return false;
                }
                true
            }
            SelectionType::Block => {
                let min_col = start.col.min(end.col);
                let max_col = start.col.max(end.col);
                point.row >= start.row
                    && point.row <= end.row
                    && point.col >= min_col
                    && point.col <= max_col
            }
            SelectionType::Lines => point.row >= start.row && point.row <= end.row,
        }
    }

    /// Check if a row is within the selection range.
    pub fn intersects_row(&self, row: i32) -> bool {
        let (start, end) = self.bounds();
        row >= start.row && row <= end.row
    }
}

/// Character classification for word boundary detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Whitespace,
    AlphaNumeric,
    Punctuation,
}

fn classify_char(c: char) -> CharClass {
    if c.is_whitespace() || c == '\0' {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::AlphaNumeric
    } else {
        CharClass::Punctuation
    }
}

/// Find the start column of the word at the given position.
///
/// Scans left from `col` until a character class boundary is found.
pub fn word_start(grid: &crate::grid::Grid, row: usize, col: usize) -> usize {
    let Some(r) = grid.row(row) else {
        return col;
    };
    let Some(cell) = r.get(col) else {
        return col;
    };
    let class = classify_char(cell.c);

    let mut start = col;
    while start > 0 {
        let Some(cell) = r.get(start - 1) else {
            break;
        };
        if classify_char(cell.c) != class {
            break;
        }
        start -= 1;
    }
    start
}

/// Find the end column (inclusive) of the word at the given position.
///
/// Scans right from `col` until a character class boundary is found.
pub fn word_end(grid: &crate::grid::Grid, row: usize, col: usize) -> usize {
    let Some(r) = grid.row(row) else {
        return col;
    };
    let Some(cell) = r.get(col) else {
        return col;
    };
    let class = classify_char(cell.c);

    let mut end = col;
    while end + 1 < r.len() {
        let Some(cell) = r.get(end + 1) else {
            break;
        };
        if classify_char(cell.c) != class {
            break;
        }
        end += 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_new() {
        let point = SelectionPoint::new(5, 10);
        let sel = Selection::new(SelectionType::Simple, point);
        assert_eq!(sel.start, sel.end);
        assert_eq!(sel.ty, SelectionType::Simple);
    }

    #[test]
    fn test_selection_update() {
        let start = SelectionPoint::new(5, 10);
        let mut sel = Selection::new(SelectionType::Simple, start);
        sel.update(SelectionPoint::new(7, 20));
        assert_eq!(sel.end, SelectionPoint::new(7, 20));
    }

    #[test]
    fn test_selection_bounds_normalized() {
        let start = SelectionPoint::new(7, 20);
        let mut sel = Selection::new(SelectionType::Simple, start);
        sel.update(SelectionPoint::new(3, 5));
        let (s, e) = sel.bounds();
        assert_eq!(s, SelectionPoint::new(3, 5));
        assert_eq!(e, SelectionPoint::new(7, 20));
    }

    #[test]
    fn test_simple_selection_contains() {
        let mut sel = Selection::new(SelectionType::Simple, SelectionPoint::new(2, 5));
        sel.update(SelectionPoint::new(4, 10));

        assert!(sel.contains(SelectionPoint::new(3, 0))); // middle row
        assert!(sel.contains(SelectionPoint::new(2, 5))); // start
        assert!(sel.contains(SelectionPoint::new(4, 10))); // end
        assert!(!sel.contains(SelectionPoint::new(2, 4))); // before start col
        assert!(!sel.contains(SelectionPoint::new(4, 11))); // after end col
        assert!(!sel.contains(SelectionPoint::new(1, 5))); // before start row
    }

    #[test]
    fn test_block_selection_contains() {
        let mut sel = Selection::new(SelectionType::Block, SelectionPoint::new(2, 5));
        sel.update(SelectionPoint::new(4, 10));

        assert!(sel.contains(SelectionPoint::new(3, 7))); // inside block
        assert!(!sel.contains(SelectionPoint::new(3, 4))); // left of block
        assert!(!sel.contains(SelectionPoint::new(3, 11))); // right of block
    }

    #[test]
    fn test_line_selection_contains() {
        let mut sel = Selection::new(SelectionType::Lines, SelectionPoint::new(2, 0));
        sel.update(SelectionPoint::new(4, 0));

        assert!(sel.contains(SelectionPoint::new(3, 50))); // any col in range
        assert!(!sel.contains(SelectionPoint::new(5, 0))); // outside range
    }

    #[test]
    fn test_intersects_row() {
        let mut sel = Selection::new(SelectionType::Simple, SelectionPoint::new(2, 0));
        sel.update(SelectionPoint::new(5, 0));

        assert!(sel.intersects_row(2));
        assert!(sel.intersects_row(3));
        assert!(sel.intersects_row(5));
        assert!(!sel.intersects_row(1));
        assert!(!sel.intersects_row(6));
    }

    #[test]
    fn test_selection_point_ordering() {
        let a = SelectionPoint::new(1, 5);
        let b = SelectionPoint::new(1, 10);
        let c = SelectionPoint::new(2, 0);
        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }

    #[test]
    fn test_word_start_middle_of_word() {
        let mut grid = crate::grid::Grid::new(1, 20);
        for (i, c) in "hello world".chars().enumerate() {
            if let Some(cell) = grid.cell_mut(0, i) {
                cell.c = c;
            }
        }
        assert_eq!(word_start(&grid, 0, 3), 0); // 'l' in "hello" -> start at 0
    }

    #[test]
    fn test_word_end_middle_of_word() {
        let mut grid = crate::grid::Grid::new(1, 20);
        for (i, c) in "hello world".chars().enumerate() {
            if let Some(cell) = grid.cell_mut(0, i) {
                cell.c = c;
            }
        }
        assert_eq!(word_end(&grid, 0, 3), 4); // 'l' in "hello" -> end at 4
    }

    #[test]
    fn test_word_start_at_beginning() {
        let mut grid = crate::grid::Grid::new(1, 20);
        for (i, c) in "hello world".chars().enumerate() {
            if let Some(cell) = grid.cell_mut(0, i) {
                cell.c = c;
            }
        }
        assert_eq!(word_start(&grid, 0, 0), 0);
    }

    #[test]
    fn test_word_boundaries_punctuation() {
        let mut grid = crate::grid::Grid::new(1, 20);
        for (i, c) in "foo::bar".chars().enumerate() {
            if let Some(cell) = grid.cell_mut(0, i) {
                cell.c = c;
            }
        }
        assert_eq!(word_start(&grid, 0, 4), 3); // ':' -> starts at 3
        assert_eq!(word_end(&grid, 0, 3), 4); // ':' -> ends at 4
        assert_eq!(word_start(&grid, 0, 5), 5); // 'b' in "bar" -> starts at 5
        assert_eq!(word_end(&grid, 0, 5), 7); // 'b' in "bar" -> ends at 7
    }

    #[test]
    fn test_word_boundaries_underscore() {
        let mut grid = crate::grid::Grid::new(1, 20);
        for (i, c) in "foo_bar baz".chars().enumerate() {
            if let Some(cell) = grid.cell_mut(0, i) {
                cell.c = c;
            }
        }
        // Underscore is part of a word
        assert_eq!(word_start(&grid, 0, 4), 0); // '_' in "foo_bar" -> starts at 0
        assert_eq!(word_end(&grid, 0, 0), 6); // 'f' in "foo_bar" -> ends at 6
    }
}
