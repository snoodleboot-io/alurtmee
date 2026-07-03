//! The desktop-notification seam.
//!
//! [`Notifier`] is the backend-agnostic trait through which the app delivers desktop
//! notifications. Keeping the delivery mechanism behind this trait is what keeps the
//! notification backend ([`crate::desktop_notifier::DesktopNotifier`]) swappable for a future
//! platform-native backend without touching the dispatch logic (ARD AD-10).
//!
//! The trait is deliberately object-safe (`&self`, no generics, no associated types) so a backend
//! could be selected at runtime as `Box<dyn Notifier>` once more than one exists.

use std::fmt;

/// A notification-delivery failure, carrying a human-readable description.
///
/// The wrapped string is for logging/diagnostics only; it never reaches a notification body and so
/// must not be relied upon to carry secrets (there are none here regardless).
#[derive(Debug)]
// integrated by the orchestrator in the poll-event handler
#[allow(dead_code)]
pub struct NotifyError(pub String);

impl fmt::Display for NotifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "notification failed: {}", self.0)
    }
}

impl std::error::Error for NotifyError {}

/// Backend-agnostic desktop-notification seam (object-safe).
///
/// Implementors deliver a `summary`/`body` pair to the platform's notification service. The shipped
/// implementation is the cross-platform [`crate::desktop_notifier::DesktopNotifier`]; any future
/// platform-native backend implements this same trait, leaving
/// [`crate::notification_dispatcher::NotificationDispatcher`] unchanged.
// integrated by the orchestrator in the poll-event handler
#[allow(dead_code)]
pub trait Notifier {
    /// Show a desktop notification with the given `summary` (title) and `body`.
    ///
    /// `body` must carry no token, secret, or PII — only repo / workflow / human reason text.
    fn notify(&self, summary: &str, body: &str) -> Result<(), NotifyError>;
}
