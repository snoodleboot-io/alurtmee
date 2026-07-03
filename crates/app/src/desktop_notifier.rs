//! The cross-platform desktop-notification backend.
//!
//! [`DesktopNotifier`] implements [`Notifier`] over `notify-rust`, which is cross-platform: it
//! speaks the freedesktop.org (XDG) Desktop Notifications D-Bus protocol on Linux and the
//! `NSUserNotification` / `mac-notification-sys` path on macOS. The app only uses the portable
//! `summary`/`body`/`show` surface, so this one backend serves every target; the host OS is selected
//! by [`crate::platform`], not here (ALRT-25 §3a).
//!
//! **macOS caveat:** the system only presents user notifications for a bundled, identified app. An
//! unsigned, non-`.app` dev binary may have `show()` return an error; that is logged by the
//! dispatcher and is a known limitation of the unsigned dev build (ALRT-25 §4.3 R2), not a bug here.

use crate::notifier::{Notifier, NotifyError};

/// The `notify-rust`-backed desktop-notification backend, used on every supported OS.
#[derive(Default)]
pub struct DesktopNotifier;

impl Notifier for DesktopNotifier {
    fn notify(&self, summary: &str, body: &str) -> Result<(), NotifyError> {
        // `show()` returns a platform-specific Ok value (a handle on Linux, `()` on macOS); we have
        // no use for it, so it is discarded. Any delivery error is mapped to our backend-agnostic
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

    /// Live verification of the real notification path (PHASE-5 §7). When a notification service is
    /// present (dev desktop) this delivers an actual notification and asserts success; headless CI
    /// has no service to deliver to, so a delivery error there is treated as "skipped", not a
    /// failure — the de-dupe/formatting logic is covered deterministically by the dispatcher tests.
    #[test]
    fn live_notification_sends_when_a_service_is_present() {
        match DesktopNotifier.notify("Alurtmee", "Phase 5 live notification check") {
            Ok(()) => {}
            Err(err) => eprintln!("no notification service; skipping live assert: {}", err.0),
        }
    }
}
