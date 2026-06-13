//! Tracing/telemetry setup for the Alurtmee binary.
//!
//! Installs a `tracing-subscriber` fmt subscriber whose verbosity is controlled by the `RUST_LOG`
//! environment variable, defaulting to `info` when it is unset. Later phases emit structured
//! `tracing` events through this subscriber.

use tracing_subscriber::EnvFilter;

/// Initialise the global `tracing` subscriber.
///
/// Reads the `RUST_LOG` environment variable for filter directives and falls back to `info` when
/// it is absent or unparseable. Call this once, as early as possible in `main`.
///
/// Convention ("no secrets in logs"): auth tokens / personal access tokens (PATs) and any other
/// credentials must NEVER be passed to `tracing` events — they would be written to stderr by this
/// subscriber. Redact or omit such values before logging.
pub fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}
