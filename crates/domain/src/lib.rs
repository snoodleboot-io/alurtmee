//! `domain` — pure types and (future) classifiers for Alurtmee.
//!
//! Per ARD AD-5/AD-7 this crate holds only platform-agnostic, I/O-free types. Classification
//! engines (human-vs-bot, feature-vs-security, slow-CI) land here in later phases; Phase 0 ships
//! the type skeleton they will operate on. One type per file (core conventions).

mod auth_state;
mod author;
mod author_kind;
mod category;
mod category_kind;
mod item;
mod org;
mod repo;
mod repo_selection;
mod user;

pub use auth_state::AuthState;
pub use author::Author;
pub use author_kind::AuthorKind;
pub use category::Category;
pub use category_kind::CategoryKind;
pub use item::Item;
pub use org::Org;
pub use repo::Repo;
pub use repo_selection::RepoSelection;
pub use user::User;
