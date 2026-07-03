//! The single OS-selection seam.
//!
//! This is the **only** module in `app` that branches on the host operating system. Every other
//! module stays platform-agnostic and depends on the abstractions exposed here (ALRT-25 §3a,
//! SOLID/DRY): call sites use [`open_url`] and the [`PlatformNotifier`] alias / [`notifier`]
//! constructor, never a concrete backend or a `#[cfg(target_os = …)]` of their own.

/// The desktop-notification backend for the host OS.
///
/// `notify-rust` is itself cross-platform — freedesktop.org D-Bus on Linux, the
/// `NSUserNotification` / `mac-notification-sys` path on macOS — so a single
/// [`crate::desktop_notifier::DesktopNotifier`] serves every target today. Should a platform ever
/// need a bespoke backend (e.g. a native macOS `UNUserNotificationCenter` impl), add a `#[cfg]` arm
/// to this alias; the [`crate::notification_dispatcher::NotificationDispatcher`] and every call site
/// stay untouched (OCP).
pub type PlatformNotifier = crate::desktop_notifier::DesktopNotifier;

/// Construct the host OS's notification backend.
pub fn notifier() -> PlatformNotifier {
    PlatformNotifier::default()
}

/// Open `url` in the user's default browser (fire-and-forget).
///
/// The launch command is the one genuinely OS-divergent value in the app, so its selection lives
/// here and nowhere else.
pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    const OPENER: &str = "open";
    #[cfg(not(target_os = "macos"))]
    const OPENER: &str = "xdg-open";

    if let Err(err) = std::process::Command::new(OPENER).arg(url).spawn() {
        tracing::warn!("could not open {url} in a browser: {err}");
    }
}
