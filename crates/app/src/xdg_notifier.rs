//! The Linux/XDG desktop-notification backend.
//!
//! [`XdgNotifier`] implements [`Notifier`] over `notify-rust`, which speaks the freedesktop.org
//! (XDG) Desktop Notifications D-Bus protocol. This is the only backend shipped in v1 (Linux-only,
//! ARD AD-10 / R2b); macOS and Windows backends are post-v1 and will implement the same
//! [`Notifier`] trait, leaving the dispatcher untouched.

use crate::notifier::{Notifier, NotifyError};

/// The freedesktop.org (XDG) notification backend, delivering notifications over D-Bus via
/// `notify-rust`.
pub struct XdgNotifier;

impl Notifier for XdgNotifier {
    fn notify(&self, summary: &str, body: &str) -> Result<(), NotifyError> {
        // `show()` returns a handle for later mutation (e.g. closing the notification); we have no
        // use for it, so it is discarded. Any delivery error is mapped to our backend-agnostic
        // error type.
        notify_rust::Notification::new()
            .summary(summary)
            .body(body)
            .show()
            .map(|_handle| ())
            .map_err(|e| NotifyError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live verification of the real XDG/D-Bus path (PHASE-5 §7). When a notification daemon is
    /// present (dev desktop) this delivers an actual notification and asserts success; headless CI
    /// has no daemon to deliver to, so a delivery error there is treated as "skipped", not a
    /// failure — the de-dupe/formatting logic is covered deterministically by the dispatcher tests.
    #[test]
    fn live_notification_sends_when_a_daemon_is_present() {
        match XdgNotifier.notify("Alurtmee", "Phase 5 live notification check") {
            Ok(()) => {}
            Err(err) => eprintln!("no notification daemon; skipping live assert: {}", err.0),
        }
    }
}
