//! Minal -- AI-first terminal emulator.
//!
//! Entry point: loads configuration and starts the application.

mod app;
mod config_watcher;
mod error;
mod event;
mod io;
#[cfg(target_os = "macos")]
mod macos;
mod mouse;
mod pane;
mod tab;
mod window;

use app::App;
use event::WakeupReason;

fn main() {
    // Initialize tracing subscriber for logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::level_filters::LevelFilter::INFO.into()),
        )
        .try_init()
        .ok(); // Silently ignore if already initialized

    tracing::info!("Starting Minal v{}", env!("CARGO_PKG_VERSION"));

    // Build the event loop.  On macOS we disable winit's built-in default menu
    // so that we can install our own fully customised menu bar afterwards.
    let mut builder = winit::event_loop::EventLoop::<WakeupReason>::with_user_event();

    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_default_menu(false);
    }

    let event_loop = match builder.build() {
        Ok(el) => el,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            std::process::exit(1);
        }
    };

    // Install the native macOS menu bar.  This must be called after the event
    // loop (and therefore NSApplication) has been initialised.
    #[cfg(target_os = "macos")]
    macos::setup_menu_bar();

    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    if let Err(e) = event_loop.run_app(&mut app) {
        tracing::error!("Event loop error: {e}");
        std::process::exit(1);
    }

    tracing::info!("Minal exited");
}
