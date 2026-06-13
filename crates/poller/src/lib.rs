//! `poller` — polling scheduler and change-detection for Alurtmee.
//!
//! Drives the two-tier polling strategy (cheap conditional change-detection + targeted
//! enrichment, ARD AD-3), diffs results against the store, and emits [`PollEvent`]s the UI
//! subscribes to. Phase 0 defines the event seam over `domain`; the async scheduler lands in
//! Phase 2.

mod poll_event;

pub use poll_event::PollEvent;
