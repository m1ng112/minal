//! Provider health status tracking.

use std::fmt;
use std::time::Instant;

/// Current status of an AI provider.
///
/// Reserved for future status-bar display integration (Phase 3 UI).
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used when status bar UI is implemented.
pub enum ProviderStatus {
    /// Provider is reachable and working.
    Available,
    /// Provider is not reachable.
    Unavailable {
        /// Reason for unavailability.
        reason: String,
        /// When the unavailability was first detected.
        since: Instant,
    },
    /// Provider is working but in degraded mode (e.g., using fallback).
    Degraded {
        /// Description of the degraded state.
        reason: String,
    },
}

impl fmt::Display for ProviderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Available => write!(f, "Available"),
            Self::Unavailable { reason, since } => {
                write!(
                    f,
                    "Unavailable: {} (for {:.0}s)",
                    reason,
                    since.elapsed().as_secs_f64()
                )
            }
            Self::Degraded { reason } => write!(f, "Degraded: {}", reason),
        }
    }
}
