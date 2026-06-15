use serde::Deserialize;

/// A label object inside a pulls list item. Only the `name` is needed for label-based
/// classification.
#[derive(Debug, Deserialize)]
pub(crate) struct WireLabel {
    pub name: String,
}
