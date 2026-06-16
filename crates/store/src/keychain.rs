//! OS keychain wrapper for the GitHub personal access tokens (PATs).
//!
//! # Why the OS keychain and not an encrypted file
//!
//! A PAT is the single highest-value secret in Alurtmee, and AD-6 (SECURITY-critical) requires it
//! to live in the OS-managed secret store, never in SQLite, config files, or logs. We delegate to
//! the OS keychain (Secret Service / Keychain Access / Windows Credential Manager) rather than
//! rolling an encrypted file because:
//!
//! - **OS-managed secret lifecycle.** The platform owns encryption-at-rest, unlock on login, and
//!   secure deletion. We never hold plaintext on disk.
//! - **Per-user isolation.** Entries are scoped to the logged-in user by the OS; another local
//!   account cannot read them.
//! - **No key-management burden.** An encrypted file would force us to derive, store, and rotate a
//!   master key — itself a secret needing the same protection, just moving the problem.
//!
//! # Multiple tokens
//!
//! Alurtmee supports more than one PAT (e.g. a personal token and a work/org token). Each is keyed
//! by a user-chosen **label** and stored under its own keychain account (`github-pat:{label}`), so
//! tokens never collide. The set of labels is tracked separately by the app (in the local config
//! DB); the keychain itself is not enumerable. [`take_legacy_token`](Keychain::take_legacy_token)
//! migrates a single-token database written before labels existed.

use keyring::{Entry, Error as KeyringError};

use crate::error::StoreError;

/// The keychain service name under which Alurtmee stores its credentials.
const SERVICE: &str = "alurtmee";

/// Prefix for the per-label account name. A token labelled `work` is stored under the keychain
/// account `github-pat:work`.
const ACCOUNT_PREFIX: &str = "github-pat:";

/// The account a single token was stored under before multi-token support (no label). Read once on
/// upgrade by [`Keychain::take_legacy_token`] and then retired.
const LEGACY_ACCOUNT: &str = "github-pat";

/// Handle to the OS keychain for Alurtmee's GitHub PATs.
///
/// Holds only the non-secret service identifier — never a token — so its derived [`Debug`] cannot
/// leak a credential. Each operation targets a specific token by its `label`.
#[derive(Debug, Clone)]
pub struct Keychain {
    service: String,
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new()
    }
}

impl Keychain {
    /// Construct a keychain handle for the production service.
    pub fn new() -> Self {
        Self {
            service: SERVICE.to_string(),
        }
    }

    /// Construct a handle against a custom service name (tests isolate behind a unique service).
    #[cfg(test)]
    pub(crate) fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry_for_account(&self, account: &str) -> Result<Entry, StoreError> {
        Ok(Entry::new(&self.service, account)?)
    }

    fn entry(&self, label: &str) -> Result<Entry, StoreError> {
        self.entry_for_account(&format!("{ACCOUNT_PREFIX}{label}"))
    }

    /// Store (or overwrite) the GitHub token for `label` in the OS keychain.
    pub fn set_token(&self, label: &str, token: &str) -> Result<(), StoreError> {
        self.entry(label)?.set_password(token)?;
        Ok(())
    }

    /// Read the GitHub token for `label`, or `None` if no entry exists.
    pub fn get_token(&self, label: &str) -> Result<Option<String>, StoreError> {
        match self.entry(label)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(other) => Err(StoreError::Keyring(other)),
        }
    }

    /// Delete the token for `label`. An absent entry is treated as success.
    pub fn delete_token(&self, label: &str) -> Result<(), StoreError> {
        match self.entry(label)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(KeyringError::NoEntry) => Ok(()),
            Err(other) => Err(StoreError::Keyring(other)),
        }
    }

    /// Read and remove a pre-multi-token credential, if one exists.
    ///
    /// Older builds stored a single token under an un-labelled account. On first launch of a
    /// multi-token build the app calls this to migrate that token to a label; returning `None` once
    /// it has been retired, so the migration runs at most once.
    pub fn take_legacy_token(&self) -> Result<Option<String>, StoreError> {
        let entry = self.entry_for_account(LEGACY_ACCOUNT)?;
        match entry.get_password() {
            Ok(token) => {
                // Best-effort retire so we never migrate twice; ignore a delete race.
                let _ = entry.delete_credential();
                Ok(Some(token))
            }
            Err(KeyringError::NoEntry) => Ok(None),
            Err(other) => Err(StoreError::Keyring(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full lifecycle against the live Secret Service. Uses a process-unique service name so
    /// concurrent runs cannot collide, and always deletes the entries before asserting so a failed
    /// assertion cannot leave a credential behind.
    #[test]
    fn labelled_tokens_round_trip_independently() {
        let service = format!("alurtmee-test-{}", std::process::id());
        let keychain = Keychain::with_service(service);

        // Absent before anything is written.
        assert_eq!(keychain.get_token("personal").expect("get absent"), None);

        keychain.set_token("personal", "tok-personal").expect("set");
        keychain.set_token("work", "tok-work").expect("set");

        let personal = keychain.get_token("personal").expect("get personal");
        let work = keychain.get_token("work").expect("get work");

        // Clean up before asserting so a failure still removes the credentials.
        keychain.delete_token("personal").expect("delete personal");
        keychain.delete_token("work").expect("delete work");

        assert_eq!(personal.as_deref(), Some("tok-personal"));
        assert_eq!(work.as_deref(), Some("tok-work"), "labels are independent");
        assert_eq!(
            keychain.get_token("personal").expect("get after delete"),
            None,
            "entry must be gone after delete"
        );
    }

    #[test]
    fn take_legacy_token_migrates_once_then_is_none() {
        let service = format!("alurtmee-legacy-test-{}", std::process::id());
        let keychain = Keychain::with_service(service.clone());

        // Seed an un-labelled legacy entry directly.
        Entry::new(&service, LEGACY_ACCOUNT)
            .expect("entry")
            .set_password("legacy-tok")
            .expect("seed legacy");

        let first = keychain.take_legacy_token().expect("take legacy");
        let second = keychain.take_legacy_token().expect("take legacy again");

        assert_eq!(first.as_deref(), Some("legacy-tok"));
        assert_eq!(second, None, "legacy token is retired after first take");
    }
}
