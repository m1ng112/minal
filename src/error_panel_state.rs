//! Error panel state for the session analysis error overlay.
//!
//! Manages visibility, animation, and scroll state for the error summary panel.

use minal_renderer::{ErrorPanelHitRegion, Viewport};

/// Animation interpolation speed (higher = faster).
const ANIMATION_SPEED: f32 = 8.0;

/// Threshold below which animation snaps to target.
const ANIMATION_EPSILON: f32 = 0.005;

/// State for the error analysis panel overlay.
pub struct ErrorPanelState {
    /// Whether the panel should be visible (animation target).
    visible: bool,
    /// Current animation progress (0.0 = hidden, 1.0 = fully visible).
    pub animation_progress: f32,
    /// Animation target (0.0 or 1.0).
    animation_target: f32,
    /// Scroll offset in pixels for the error list.
    pub scroll_offset: f32,
    /// Panel height as fraction of window height.
    pub panel_height_ratio: f32,
    /// Cached hit regions from the last render (used for future mouse handling).
    #[allow(dead_code)]
    pub hit_regions: Vec<ErrorPanelHitRegion>,
}

impl ErrorPanelState {
    /// Creates a new error panel state.
    pub fn new(panel_height_ratio: f32) -> Self {
        Self {
            visible: false,
            animation_progress: 0.0,
            animation_target: 0.0,
            scroll_offset: 0.0,
            panel_height_ratio,
            hit_regions: Vec::new(),
        }
    }

    /// Toggles the panel open/closed.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        self.animation_target = if self.visible { 1.0 } else { 0.0 };
    }

    /// Whether the panel is visible or animating toward visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Whether the animation is still in progress.
    pub fn is_animating(&self) -> bool {
        (self.animation_progress - self.animation_target).abs() > ANIMATION_EPSILON
    }

    /// Updates animation progress. Returns `true` if a redraw is needed.
    pub fn update_animation(&mut self, dt: f32) -> bool {
        if !self.is_animating() {
            return false;
        }
        let diff = self.animation_target - self.animation_progress;
        self.animation_progress += diff * (ANIMATION_SPEED * dt).min(1.0);
        if (self.animation_progress - self.animation_target).abs() < ANIMATION_EPSILON {
            self.animation_progress = self.animation_target;
        }
        true
    }

    /// Computes the panel viewport in screen coordinates.
    pub fn panel_viewport(
        &self,
        screen_width: f32,
        screen_height: f32,
        top_offset: f32,
    ) -> Viewport {
        let available_height = screen_height - top_offset;
        let panel_h = available_height * self.panel_height_ratio * self.animation_progress;
        let y = screen_height - panel_h;
        Viewport {
            x: 0.0,
            y,
            width: screen_width,
            height: panel_h,
        }
    }

    /// Scroll up by the given number of pixels.
    pub fn scroll_up(&mut self, pixels: f32) {
        self.scroll_offset = (self.scroll_offset + pixels).max(0.0);
    }

    /// Scroll down by the given number of pixels.
    pub fn scroll_down(&mut self, pixels: f32) {
        self.scroll_offset = (self.scroll_offset - pixels).max(0.0);
    }

    /// Whether the panel is fully hidden (animation complete at 0.0).
    pub fn is_fully_hidden(&self) -> bool {
        self.animation_progress < ANIMATION_EPSILON && self.animation_target < ANIMATION_EPSILON
    }

    /// Close the panel.
    pub fn close(&mut self) {
        self.visible = false;
        self.animation_target = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_panel_is_hidden() {
        let panel = ErrorPanelState::new(0.4);
        assert!(!panel.is_visible());
        assert!(panel.is_fully_hidden());
    }

    #[test]
    fn toggle_makes_visible() {
        let mut panel = ErrorPanelState::new(0.4);
        panel.toggle();
        assert!(panel.is_visible());
        assert!(panel.is_animating());
    }

    #[test]
    fn double_toggle_hides() {
        let mut panel = ErrorPanelState::new(0.4);
        panel.toggle();
        panel.toggle();
        assert!(!panel.is_visible());
    }

    #[test]
    fn animation_progresses() {
        let mut panel = ErrorPanelState::new(0.4);
        panel.toggle();
        for _ in 0..100 {
            panel.update_animation(0.016);
        }
        assert!(!panel.is_animating());
        assert!((panel.animation_progress - 1.0).abs() < 0.01);
    }

    #[test]
    fn scroll_operations() {
        let mut panel = ErrorPanelState::new(0.4);
        panel.scroll_up(50.0);
        assert_eq!(panel.scroll_offset, 50.0);
        panel.scroll_down(30.0);
        assert_eq!(panel.scroll_offset, 20.0);
        panel.scroll_down(100.0);
        assert_eq!(panel.scroll_offset, 0.0);
    }

    #[test]
    fn panel_viewport_calculation() {
        let mut panel = ErrorPanelState::new(0.4);
        panel.animation_progress = 1.0;
        let vp = panel.panel_viewport(800.0, 600.0, 28.0);
        let expected_height = (600.0 - 28.0) * 0.4;
        assert!((vp.height - expected_height).abs() < 0.1);
        assert!((vp.y - (600.0 - expected_height)).abs() < 0.1);
    }

    #[test]
    fn close_hides_panel() {
        let mut panel = ErrorPanelState::new(0.4);
        panel.toggle();
        assert!(panel.is_visible());
        panel.close();
        assert!(!panel.is_visible());
    }
}
