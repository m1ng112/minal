//! Minal — AI-first terminal emulator.
//!
//! Entry point: loads configuration and starts the application.

fn main() {
    // Initialize tracing subscriber for logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::level_filters::LevelFilter::INFO.into()),
        )
        .init();

    tracing::info!("Starting Minal v{}", env!("CARGO_PKG_VERSION"));

    // TODO: Load config, create window, start event loop
    tracing::info!("Minal initialized successfully");
}
