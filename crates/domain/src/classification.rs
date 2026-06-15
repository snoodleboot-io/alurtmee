use serde::{Deserialize, Serialize};

use crate::author_kind::AuthorKind;
use crate::category::Category;
use crate::pr_id::PrId;

/// The classification verdict for a PR: who authored it (human/bot) and what kind of change it is.
///
/// Derived fresh each time a PR changes (the classifier is pure and cheap), so it is delivered as
/// an event rather than persisted — the durable input is the user *correction* (stored separately),
/// which the next classification reads back. `category.signal` records which layer fired, so the UI
/// can explain *why* a PR was tagged the way it was.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    /// Which PR this verdict belongs to.
    pub id: PrId,
    /// Human vs bot.
    pub author_kind: AuthorKind,
    /// Feature vs security vs unknown, with the firing signal and confidence.
    pub category: Category,
}
