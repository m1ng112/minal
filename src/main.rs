//! Minal -- AI-first terminal emulator.
//!
//! Entry point: loads configuration and starts the application.

mod app;
mod error;
mod window;

use app::App;

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

    let event_loop = match winit::event_loop::EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            std::process::exit(1);
        }
    };

    let mut app = App::new();
    if let Err(e) = event_loop.run_app(&mut app) {
        tracing::error!("Event loop error: {e}");
        std::process::exit(1);
    }

    tracing::info!("Minal exited");
}
