//! `domain` — pure types and (future) classifiers for Alurtmee.
//!
//! Per ARD AD-5/AD-7 this crate holds only platform-agnostic, I/O-free types. Classification
//! engines (human-vs-bot, feature-vs-security, slow-CI) land here in later phases; Phase 0 ships
//! the type skeleton they will operate on. One type per file (core conventions).

mod author;
mod author_kind;
mod category;
mod category_kind;
mod item;

pub use author::Author;
pub use author_kind::AuthorKind;
pub use category::Category;
pub use category_kind::CategoryKind;
pub use item::Item;
