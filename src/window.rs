//! Window creation and management.

use std::sync::Arc;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// Creates a new Minal window with the given parameters.
///
/// The window is created via `ActiveEventLoop` during the `resumed`
/// callback, as required by winit 0.30.
pub fn create_window(
    event_loop: &ActiveEventLoop,
    title: &str,
    width: u32,
    height: u32,
) -> Result<Arc<Window>, crate::error::AppError> {
    let attrs = Window::default_attributes()
        .with_title(title)
        .with_inner_size(winit::dpi::LogicalSize::new(width, height))
        .with_min_inner_size(winit::dpi::LogicalSize::new(400u32, 300u32));

    let window = event_loop.create_window(attrs)?;
    Ok(Arc::new(window))
}
