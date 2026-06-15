//! `domain` — pure types and (future) classifiers for Alurtmee.
//!
//! Per ARD AD-5/AD-7 this crate holds only platform-agnostic, I/O-free types. Classification
//! engines (human-vs-bot, feature-vs-security, slow-CI) land here in later phases; Phase 0 ships
//! the type skeleton they will operate on. One type per file (core conventions).

mod auth_state;
mod author;
mod author_kind;
mod bot_overrides;
mod category;
mod category_classifier;
mod category_kind;
mod change_event;
mod check_run;
mod classification;
mod classification_input;
mod comment;
mod comment_kind;
mod item;
mod label_map;
mod org;
mod poll_cadence;
mod pr_enrichment;
mod pr_id;
mod pull_request;
mod rate_limit_state;
mod repo;
mod repo_selection;
mod review;
mod test_state;
mod test_summary;
mod user;

pub use auth_state::AuthState;
pub use author::Author;
pub use author_kind::AuthorKind;
pub use bot_overrides::BotOverrides;
pub use category::Category;
pub use category_classifier::classify_category;
pub use category_kind::CategoryKind;
pub use change_event::ChangeEvent;
pub use check_run::CheckRun;
pub use classification::Classification;
pub use classification_input::ClassificationInput;
pub use comment::Comment;
pub use comment_kind::CommentKind;
pub use item::Item;
pub use label_map::LabelMap;
pub use org::Org;
pub use poll_cadence::PollCadence;
pub use pr_enrichment::PrEnrichment;
pub use pr_id::PrId;
pub use pull_request::PullRequest;
pub use rate_limit_state::RateLimitState;
pub use repo::Repo;
pub use repo_selection::RepoSelection;
pub use review::Review;
pub use test_state::TestState;
pub use test_summary::TestSummary;
pub use user::User;
