//! App-level mouse state machine and coordinate conversion.

use std::time::Instant;

/// Multi-click timeout in milliseconds.
const MULTI_CLICK_TIMEOUT_MS: u64 = 500;

/// Maximum distance (in cells) for a click to count as a multi-click.
const MULTI_CLICK_MAX_DISTANCE: usize = 3;

/// Tracks mouse state for the main thread event loop.
pub struct MouseState {
    /// Last known cursor position in pixels.
    pub pixel_pos: (f64, f64),
    /// Last computed cell position (col, row).
    pub cell_pos: (usize, usize),
    /// Whether the left button is currently pressed.
    pub left_pressed: bool,
    /// Click count for multi-click detection (1=single, 2=double, 3=triple).
    pub click_count: u8,
    /// Timestamp of last click.
    last_click_time: Instant,
    /// Cell position of last click (col, row).
    last_click_pos: (usize, usize),
}

impl Default for MouseState {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseState {
    /// Creates a new mouse state with default values.
    pub fn new() -> Self {
        Self {
            pixel_pos: (0.0, 0.0),
            cell_pos: (0, 0),
            left_pressed: false,
            click_count: 0,
            last_click_time: Instant::now(),
            last_click_pos: (0, 0),
        }
    }

    /// Convert pixel coordinates to cell coordinates (col, row), clamped to grid bounds.
    pub fn pixel_to_cell(
        px: f64,
        py: f64,
        cell_width: f32,
        cell_height: f32,
        padding: f32,
        max_cols: usize,
        max_rows: usize,
    ) -> (usize, usize) {
        let x = (px as f32 - padding).max(0.0);
        let y = (py as f32 - padding).max(0.0);
        let col = if cell_width > 0.0 {
            (x / cell_width) as usize
        } else {
            0
        };
        let row = if cell_height > 0.0 {
            (y / cell_height) as usize
        } else {
            0
        };
        (
            col.min(max_cols.saturating_sub(1)),
            row.min(max_rows.saturating_sub(1)),
        )
    }

    /// Register a click and return the click count (1, 2, or 3).
    pub fn register_click(&mut self, col: usize, row: usize) -> u8 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_click_time).as_millis() as u64;
        let distance = col.abs_diff(self.last_click_pos.0) + row.abs_diff(self.last_click_pos.1);

        if elapsed < MULTI_CLICK_TIMEOUT_MS && distance <= MULTI_CLICK_MAX_DISTANCE {
            self.click_count = if self.click_count >= 3 {
                1
            } else {
                self.click_count + 1
            };
        } else {
            self.click_count = 1;
        }

        self.last_click_time = now;
        self.last_click_pos = (col, row);
        self.click_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_to_cell_basic() {
        // 10px wide, 20px tall cells, 5px padding
        let (col, row) = MouseState::pixel_to_cell(25.0, 45.0, 10.0, 20.0, 5.0, 80, 24);
        // x = 25 - 5 = 20, col = 20/10 = 2
        // y = 45 - 5 = 40, row = 40/20 = 2
        assert_eq!(col, 2);
        assert_eq!(row, 2);
    }

    #[test]
    fn test_pixel_to_cell_clamped() {
        let (col, row) = MouseState::pixel_to_cell(10000.0, 10000.0, 10.0, 20.0, 5.0, 80, 24);
        assert_eq!(col, 79);
        assert_eq!(row, 23);
    }

    #[test]
    fn test_pixel_to_cell_negative_offset() {
        // Coordinates inside padding area
        let (col, row) = MouseState::pixel_to_cell(2.0, 2.0, 10.0, 20.0, 5.0, 80, 24);
        assert_eq!(col, 0);
        assert_eq!(row, 0);
    }

    #[test]
    fn test_pixel_to_cell_zero_cell_size() {
        let (col, row) = MouseState::pixel_to_cell(100.0, 100.0, 0.0, 0.0, 5.0, 80, 24);
        assert_eq!(col, 0);
        assert_eq!(row, 0);
    }

    #[test]
    fn test_register_click_single() {
        let mut state = MouseState::new();
        let count = state.register_click(5, 5);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_register_click_double() {
        let mut state = MouseState::new();
        state.register_click(5, 5);
        let count = state.register_click(5, 5);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_register_click_triple() {
        let mut state = MouseState::new();
        state.register_click(5, 5);
        state.register_click(5, 5);
        let count = state.register_click(5, 5);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_register_click_wraps_after_triple() {
        let mut state = MouseState::new();
        state.register_click(5, 5);
        state.register_click(5, 5);
        state.register_click(5, 5);
        let count = state.register_click(5, 5);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_register_click_resets_on_distance() {
        let mut state = MouseState::new();
        state.register_click(5, 5);
        // Click far away should reset to single
        let count = state.register_click(50, 50);
        assert_eq!(count, 1);
    }
}
