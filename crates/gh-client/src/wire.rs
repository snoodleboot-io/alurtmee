//! Wire DTOs — the anti-corruption layer between GitHub's JSON and the pure `domain` types.
//!
//! GitHub's payloads are wide and nested: a repository carries dozens of fields and an `owner`
//! *object*, where `domain::Repo` wants a flat, persistence-friendly value. If we deserialized
//! `domain` types directly from GitHub JSON, every quirk of GitHub's schema (extra fields, nesting,
//! renames) would leak into the pure domain layer and couple it to an external API.
//!
//! Instead we deserialize into private DTOs (which mirror GitHub's shape and tolerate extra fields
//! via serde's default of ignoring unknowns) and map them through `From` impls. The domain types
//! stay clean; GitHub's nesting and extras stop here. One type per file (core conventions).

mod wire_org;
mod wire_repo;
mod wire_repo_owner;
mod wire_user;

pub(crate) use wire_org::WireOrg;
pub(crate) use wire_repo::WireRepo;
pub(crate) use wire_user::WireUser;
