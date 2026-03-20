//! Minal -- AI-first terminal emulator.
//!
//! Entry point: loads configuration and starts the application.

mod app;
mod config_watcher;
mod error;
mod event;
mod io;
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

    let event_loop = match winit::event_loop::EventLoop::<WakeupReason>::with_user_event().build() {
        Ok(el) => el,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            std::process::exit(1);
        }
    };

    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    if let Err(e) = event_loop.run_app(&mut app) {
        tracing::error!("Event loop error: {e}");
        std::process::exit(1);
    }

    tracing::info!("Minal exited");
}
