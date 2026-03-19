//! Config file watcher for theme hot-reload.
//!
//! Watches the config file's parent directory for modifications and
//! reloads the theme when changes are detected. Uses timestamp-based
//! debouncing to coalesce rapid filesystem events.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use winit::event_loop::EventLoopProxy;

use crate::event::WakeupReason;

/// Minimum interval between config reloads (debounce window).
const DEBOUNCE_DURATION: Duration = Duration::from_millis(200);

/// Watches the configuration file for changes and sends theme updates
/// to the main event loop when the config is modified.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    shutdown: Arc<AtomicBool>,
}

impl ConfigWatcher {
    /// Create a new config watcher for the given config file path.
    ///
    /// The watcher monitors the parent directory (to catch atomic rename
    /// writes by text editors) and reloads the full config on modification.
    /// Only theme changes are applied; other config changes require a restart.
    ///
    /// # Errors
    /// Returns a `notify::Error` if the watcher cannot be created or
    /// the parent directory cannot be watched.
    pub fn new(
        config_path: PathBuf,
        proxy: EventLoopProxy<WakeupReason>,
    ) -> Result<Self, notify::Error> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let path = config_path.clone();
        let last_reload: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if shutdown_clone.load(Ordering::Relaxed) {
                return;
            }
            if let Ok(event) = res {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) => {
                        // Timestamp-based debounce: skip if we reloaded recently.
                        let now = Instant::now();
                        let should_reload = {
                            let guard = last_reload.lock().unwrap_or_else(|p| p.into_inner());
                            guard.is_none_or(|t| now.duration_since(t) >= DEBOUNCE_DURATION)
                        };
                        if !should_reload {
                            return;
                        }

                        match minal_config::Config::load_from(&path) {
                            Ok(config) => {
                                // Update the last reload timestamp.
                                if let Ok(mut guard) =
                                    last_reload.lock().map_err(|p| p.into_inner())
                                {
                                    *guard = Some(now);
                                }
                                let theme = config.colors;
                                let _ =
                                    proxy.send_event(WakeupReason::ThemeChanged(Box::new(theme)));
                                tracing::info!(
                                    "Config reloaded (only theme changes applied without restart)"
                                );
                            }
                            Err(e) => {
                                tracing::warn!("Failed to reload config: {e}");
                            }
                        }
                    }
                    _ => {}
                }
            }
        })?;

        // Watch the parent directory to catch atomic rename writes by editors.
        if let Some(parent) = config_path.parent() {
            watcher.watch(parent, RecursiveMode::NonRecursive)?;
        }

        Ok(Self {
            _watcher: watcher,
            shutdown,
        })
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
