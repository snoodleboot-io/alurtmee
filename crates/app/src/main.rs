//! Alurtmee desktop application entry point.
//!
//! Phase 0 stands up a real, runnable Iced window with no behaviour yet. The poller→store→UI
//! pipeline and notifications attach in later phases.
//!
//! **Why Iced's Elm/`application` model fits here (MASTER §3.6):** Alurtmee is idle most of the
//! time — it polls on a slow cadence and the UI only needs to change when a poll produces a new
//! event. Iced is retained-mode and redraws *only in response to a `Message`*, so an idle
//! dashboard costs ~no CPU between updates (NFR2), unlike an immediate-mode toolkit that repaints
//! every frame. The unidirectional `state → view → message → update` loop also maps cleanly onto
//! "poller emits events → state updates → widgets redraw", which is exactly the data flow in
//! ARD AD-7. That is why this is the right model, not merely "we picked Iced".

mod telemetry;

use iced::widget::{center, text};
use iced::Element;

/// Application state. Empty in Phase 0; gains the dashboard model in later phases.
#[derive(Default)]
struct Alurtmee;

/// Messages that drive state transitions. None exist yet — the window is static in Phase 0.
#[derive(Debug, Clone)]
enum Message {}

impl Alurtmee {
    fn update(&mut self, message: Message) {
        // No messages are produced in Phase 0; this match is exhaustive over an empty enum.
        match message {}
    }

    fn view(&self) -> Element<'_, Message> {
        center(text("Alurtmee")).into()
    }
}

fn main() -> iced::Result {
    telemetry::init();
    tracing::info!("starting alurtmee");
    iced::application("Alurtmee", Alurtmee::update, Alurtmee::view).run()
}
