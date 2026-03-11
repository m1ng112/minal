//! Scrollback history buffer using a ring-buffer implementation.

use crate::grid::Row;

/// Ring-buffer-based scrollback history.
///
/// Stores rows that have been scrolled off the top of the terminal screen.
/// The buffer has a configurable maximum capacity; when full, the oldest
/// rows are discarded to make room for new entries.
#[derive(Debug, Clone)]
pub struct Scrollback {
    /// The ring buffer storage.
    buffer: Vec<Row>,
    /// Index of the oldest entry in the ring buffer.
    head: usize,
    /// Number of valid entries in the buffer.
    len: usize,
    /// Maximum number of rows to store.
    capacity: usize,
}

impl Scrollback {
    /// Create a new scrollback buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity.min(1024)),
            head: 0,
            len: 0,
            capacity,
        }
    }

    /// Number of rows in the scrollback buffer.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the scrollback buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Maximum capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Push a row into the scrollback buffer.
    ///
    /// If the buffer is at capacity, the oldest row is discarded.
    pub fn push(&mut self, row: Row) {
        if self.capacity == 0 {
            return;
        }

        if self.buffer.len() < self.capacity {
            // Buffer not yet fully allocated
            self.buffer.push(row);
            self.len = self.buffer.len();
        } else {
            // Overwrite the oldest entry
            self.buffer[self.head] = row;
            self.head = (self.head + 1) % self.capacity;
            // len stays at capacity
        }
    }

    /// Push multiple rows into the scrollback buffer.
    pub fn push_rows(&mut self, rows: Vec<Row>) {
        for row in rows {
            self.push(row);
        }
    }

    /// Get a row by index (0 = oldest, len-1 = newest).
    pub fn get(&self, index: usize) -> Option<&Row> {
        if index >= self.len {
            return None;
        }
        if self.buffer.len() < self.capacity {
            // Not yet wrapped
            self.buffer.get(index)
        } else {
            let actual = (self.head + index) % self.capacity;
            self.buffer.get(actual)
        }
    }

    /// Iterate over rows from oldest to newest.
    pub fn iter(&self) -> ScrollbackIter<'_> {
        ScrollbackIter {
            scrollback: self,
            index: 0,
        }
    }

    /// Clear the scrollback buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.head = 0;
        self.len = 0;
    }

    /// Pop the most recent row from the buffer.
    ///
    /// Returns `None` if the buffer is empty.
    pub fn pop(&mut self) -> Option<Row> {
        if self.len == 0 {
            return None;
        }

        if self.buffer.len() < self.capacity || self.len < self.capacity {
            self.len -= 1;
            Some(self.buffer.remove(self.len))
        } else {
            // Ring buffer is full: newest entry is at (head - 1 + capacity) % capacity
            let newest = (self.head + self.capacity - 1) % self.capacity;
            let row = self.buffer[newest].clone();
            self.len -= 1;
            // Adjust: just decrease len; head stays, we won't read past len
            if newest == 0 {
                self.head = self.head.min(self.len);
            }
            Some(row)
        }
    }
}

/// Iterator over scrollback rows, oldest to newest.
pub struct ScrollbackIter<'a> {
    scrollback: &'a Scrollback,
    index: usize,
}

impl<'a> Iterator for ScrollbackIter<'a> {
    type Item = &'a Row;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.scrollback.get(self.index)?;
        self.index += 1;
        Some(row)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.scrollback.len().saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for ScrollbackIter<'a> {}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(c: char, cols: usize) -> Row {
        let mut row = Row::new(cols);
        if let Some(cell) = row.get_mut(0) {
            cell.c = c;
        }
        row
    }

    #[test]
    fn test_push_and_get() {
        let mut sb = Scrollback::new(10);
        sb.push(make_row('A', 80));
        sb.push(make_row('B', 80));
        assert_eq!(sb.len(), 2);
        assert_eq!(sb.get(0).unwrap().get(0).unwrap().c, 'A');
        assert_eq!(sb.get(1).unwrap().get(0).unwrap().c, 'B');
    }

    #[test]
    fn test_capacity_limit() {
        let mut sb = Scrollback::new(3);
        sb.push(make_row('A', 10));
        sb.push(make_row('B', 10));
        sb.push(make_row('C', 10));
        sb.push(make_row('D', 10));
        assert_eq!(sb.len(), 3);
        // 'A' was discarded, oldest is now 'B'
        assert_eq!(sb.get(0).unwrap().get(0).unwrap().c, 'B');
        assert_eq!(sb.get(1).unwrap().get(0).unwrap().c, 'C');
        assert_eq!(sb.get(2).unwrap().get(0).unwrap().c, 'D');
    }

    #[test]
    fn test_zero_capacity() {
        let mut sb = Scrollback::new(0);
        sb.push(make_row('A', 10));
        assert_eq!(sb.len(), 0);
        assert!(sb.is_empty());
    }

    #[test]
    fn test_clear() {
        let mut sb = Scrollback::new(10);
        sb.push(make_row('A', 10));
        sb.push(make_row('B', 10));
        sb.clear();
        assert!(sb.is_empty());
    }

    #[test]
    fn test_iter() {
        let mut sb = Scrollback::new(10);
        sb.push(make_row('A', 10));
        sb.push(make_row('B', 10));
        sb.push(make_row('C', 10));

        let chars: Vec<char> = sb.iter().map(|r| r.get(0).unwrap().c).collect();
        assert_eq!(chars, vec!['A', 'B', 'C']);
    }

    #[test]
    fn test_iter_after_wrap() {
        let mut sb = Scrollback::new(3);
        for c in ['A', 'B', 'C', 'D', 'E'] {
            sb.push(make_row(c, 10));
        }

        let chars: Vec<char> = sb.iter().map(|r| r.get(0).unwrap().c).collect();
        assert_eq!(chars, vec!['C', 'D', 'E']);
    }

    #[test]
    fn test_push_rows() {
        let mut sb = Scrollback::new(10);
        let rows = vec![make_row('A', 10), make_row('B', 10)];
        sb.push_rows(rows);
        assert_eq!(sb.len(), 2);
    }

    #[test]
    fn test_out_of_bounds_get() {
        let sb = Scrollback::new(10);
        assert!(sb.get(0).is_none());
    }
}
