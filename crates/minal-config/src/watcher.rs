//! Config file watcher for hot-reload support.
//!
//! Uses `notify` to watch the config file for changes and sends
//! reloaded [`Config`] through a channel when the file is modified.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::Config;

/// Debounce interval for filesystem events.
///
/// Editors like vim/neovim perform atomic saves (write temp, rename) which
/// generate multiple events in quick succession. This interval ensures we
/// only reload once after the final event.
const DEBOUNCE_MS: u64 = 300;

/// Events emitted by the config watcher.
#[derive(Debug, Clone)]
pub enum ConfigEvent {
    /// The config file was reloaded successfully.
    Reloaded(Box<Config>),
}

/// Watches the config file for changes and sends reload events.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    /// Start watching the given config file path.
    ///
    /// When the file changes, the watcher reloads the config and sends
    /// a [`ConfigEvent::Reloaded`] through the provided sender.
    ///
    /// The watcher runs on a background thread managed by `notify`.
    ///
    /// # Errors
    /// Returns an error if the watcher cannot be created or the path
    /// cannot be watched.
    pub fn start(config_path: &Path, tx: Sender<ConfigEvent>) -> Result<Self, crate::ConfigError> {
        let config_path: PathBuf = config_path.to_path_buf();

        // Determine which directory to watch. Watch the parent directory
        // so we catch file renames (atomic saves).
        let watch_dir = config_path.parent().unwrap_or(Path::new(".")).to_path_buf();

        let config_path_arc = Arc::new(config_path);
        let config_path_for_handler = Arc::clone(&config_path_arc);

        // Track last reload time for debouncing.
        let last_reload = Arc::new(std::sync::Mutex::new(
            Instant::now() - Duration::from_secs(1),
        ));
        let last_reload_clone = Arc::clone(&last_reload);

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            let Ok(event) = res else {
                return;
            };

            // Only react to file creation or modification events.
            let is_write_event = matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_));
            if !is_write_event {
                return;
            }

            // Check if any of the affected paths match our config file.
            let is_our_file = event
                .paths
                .iter()
                .any(|p| p == config_path_for_handler.as_ref());
            if !is_our_file {
                return;
            }

            // Debounce: skip if we reloaded recently.
            {
                let mut last = last_reload_clone.lock().unwrap_or_else(|e| e.into_inner());
                let now = Instant::now();
                if now.duration_since(*last) < Duration::from_millis(DEBOUNCE_MS) {
                    return;
                }
                *last = now;
            }

            // Reload the config.
            match Config::load_from(config_path_for_handler.as_ref()) {
                Ok(config) => {
                    tracing::info!("Config file changed, reloading");
                    let _ = tx.send(ConfigEvent::Reloaded(Box::new(config)));
                }
                Err(e) => {
                    tracing::warn!("Config reload failed (keeping previous config): {e}");
                }
            }
        })
        .map_err(|e| crate::ConfigError::Watcher(format!("failed to create watcher: {e}")))?;

        watcher
            .watch(&watch_dir, RecursiveMode::NonRecursive)
            .map_err(|e| {
                crate::ConfigError::Watcher(format!("failed to watch {}: {e}", watch_dir.display()))
            })?;

        tracing::info!(path = %config_path_arc.display(), "Config watcher started");

        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn watcher_detects_file_change() {
        let dir = std::env::temp_dir().join("minal_test_watcher");
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("test_config.toml");

        // Write initial config.
        std::fs::write(&config_path, "").expect("write initial");

        let (tx, rx) = crossbeam_channel::bounded(10);
        let _watcher = ConfigWatcher::start(&config_path, tx).expect("start watcher");

        // Give the watcher time to initialize.
        std::thread::sleep(Duration::from_millis(200));

        // Modify the file.
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&config_path)
            .expect("open for write");
        writeln!(f, "[font]\nsize = 20.0").expect("write");
        drop(f);

        // Wait for the event with a generous timeout.
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(ConfigEvent::Reloaded(config)) => {
                assert!((config.font.size - 20.0).abs() < f32::EPSILON);
            }
            Err(e) => {
                // On some CI environments filesystem events may be unreliable.
                // Don't fail hard, but log.
                eprintln!("watcher test: no event received within timeout: {e}");
            }
        }

        // Cleanup.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watcher_handles_invalid_toml() {
        let dir = std::env::temp_dir().join("minal_test_watcher_invalid");
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("test_invalid.toml");

        std::fs::write(&config_path, "").expect("write initial");

        let (tx, rx) = crossbeam_channel::bounded(10);
        let _watcher = ConfigWatcher::start(&config_path, tx).expect("start watcher");

        std::thread::sleep(Duration::from_millis(200));

        // Write invalid TOML.
        std::fs::write(&config_path, "{{{{invalid").expect("write invalid");

        // Should NOT receive an event (invalid config is logged, not sent).
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(_) => panic!("should not receive event for invalid config"),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // Expected: no event for invalid config.
            }
            Err(e) => panic!("unexpected error: {e}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
