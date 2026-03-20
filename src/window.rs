//! Window creation and management.

use std::sync::Arc;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// Creates a new Minal window with the given parameters.
///
/// The window is created via `ActiveEventLoop` during the `resumed`
/// callback, as required by winit 0.30.
///
/// On macOS, additional platform-specific attributes are applied:
/// - Transparent titlebar with full-size content view (modern look)
/// - Window is movable by dragging the background
/// - Option key treated as Alt according to `macos_config.option_as_alt`
/// - IME input is enabled for CJK and other input method support
pub fn create_window(
    event_loop: &ActiveEventLoop,
    title: &str,
    width: u32,
    height: u32,
    macos_config: &minal_config::MacosConfig,
) -> Result<Arc<Window>, crate::error::AppError> {
    let attrs = Window::default_attributes()
        .with_title(title)
        .with_inner_size(winit::dpi::LogicalSize::new(width, height))
        .with_min_inner_size(winit::dpi::LogicalSize::new(400u32, 300u32));

    #[cfg(target_os = "macos")]
    let attrs = {
        use winit::platform::macos::WindowAttributesExtMacOS;

        let winit_option_as_alt = match macos_config.option_as_alt {
            minal_config::OptionAsAlt::Left => winit::platform::macos::OptionAsAlt::OnlyLeft,
            minal_config::OptionAsAlt::Right => winit::platform::macos::OptionAsAlt::OnlyRight,
            minal_config::OptionAsAlt::Both => winit::platform::macos::OptionAsAlt::Both,
            minal_config::OptionAsAlt::None => winit::platform::macos::OptionAsAlt::None,
        };

        attrs
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true)
            .with_movable_by_window_background(true)
            .with_accepts_first_mouse(true)
            .with_option_as_alt(winit_option_as_alt)
    };

    // Suppress unused variable warning on non-macOS.
    #[cfg(not(target_os = "macos"))]
    let _ = macos_config;

    let window = event_loop.create_window(attrs)?;

    // Enable IME input for multi-byte character composition (CJK, etc.).
    window.set_ime_allowed(true);

    Ok(Arc::new(window))
}
