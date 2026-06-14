//! OS keychain wrapper for the GitHub personal access token (PAT).
//!
//! # Why the OS keychain and not an encrypted file
//!
//! The PAT is the single highest-value secret in Alurtmee, and AD-6 (SECURITY-critical) requires it
//! to live in the OS-managed secret store, never in SQLite, config files, or logs. We delegate to
//! the OS keychain (Secret Service / Keychain Access / Windows Credential Manager) rather than
//! rolling an encrypted file because:
//!
//! - **OS-managed secret lifecycle.** The platform owns encryption-at-rest, unlock on login, and
//!   secure deletion. We never hold plaintext on disk.
//! - **Per-user isolation.** Entries are scoped to the logged-in user by the OS; another local
//!   account cannot read them.
//! - **No key-management burden.** An encrypted file would force us to derive, store, and rotate a
//!   master key — itself a secret needing the same protection, just moving the problem. The keychain
//!   removes that bootstrap entirely.
//!
//! This is the mechanism that upholds the privacy invariant: the token is reachable only through
//! the live OS keychain session, and this type deliberately never keeps the token in memory beyond
//! the call that uses it.

use keyring::{Entry, Error as KeyringError};

use crate::error::StoreError;

/// The keychain service name under which Alurtmee stores its credentials.
const SERVICE: &str = "alurtmee";

/// The account/username the PAT is stored under within the service.
const ACCOUNT: &str = "github-pat";

/// Handle to the OS keychain entry holding the GitHub PAT.
///
/// Holds only the non-secret service/account identifiers — never the token itself — so its derived
/// [`Debug`] cannot leak a credential.
#[derive(Debug, Clone)]
pub struct Keychain {
    service: String,
    account: String,
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new()
    }
}

impl Keychain {
    /// Construct a keychain handle for the production service/account.
    pub fn new() -> Self {
        Self {
            service: SERVICE.to_string(),
            account: ACCOUNT.to_string(),
        }
    }

    /// Construct a handle against a custom service name (tests isolate behind a unique service).
    #[cfg(test)]
    pub(crate) fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            account: ACCOUNT.to_string(),
        }
    }

    fn entry(&self) -> Result<Entry, StoreError> {
        Ok(Entry::new(&self.service, &self.account)?)
    }

    /// Store (or overwrite) the GitHub token in the OS keychain.
    pub fn set_token(&self, token: &str) -> Result<(), StoreError> {
        self.entry()?.set_password(token)?;
        Ok(())
    }

    /// Read the GitHub token from the OS keychain, or `None` if no entry exists yet.
    pub fn get_token(&self) -> Result<Option<String>, StoreError> {
        match self.entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(other) => Err(StoreError::Keyring(other)),
        }
    }

    /// Delete the GitHub token from the OS keychain. Absent entry is treated as success.
    pub fn delete_token(&self) -> Result<(), StoreError> {
        match self.entry()?.delete_credential() {
            Ok(()) => Ok(()),
            Err(KeyringError::NoEntry) => Ok(()),
            Err(other) => Err(StoreError::Keyring(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full lifecycle against the live Secret Service. Uses a process-unique service name so
    /// concurrent runs cannot collide, and always deletes the entry before asserting so a failed
    /// assertion cannot leave a credential behind.
    #[test]
    fn token_round_trip_against_live_keychain() {
        let service = format!("alurtmee-test-{}", std::process::id());
        let keychain = Keychain::with_service(service);
        let dummy = "dummy-token-not-a-real-pat";

        // Absent before anything is written.
        assert_eq!(
            keychain.get_token().expect("get absent token"),
            None,
            "fresh service should have no entry"
        );

        keychain.set_token(dummy).expect("set token");
        let read = keychain.get_token().expect("get token after set");

        // Clean up before asserting on the read so a failure here still removes the credential.
        keychain.delete_token().expect("delete token");

        assert_eq!(read.as_deref(), Some(dummy));
        assert_eq!(
            keychain.get_token().expect("get after delete"),
            None,
            "entry must be gone after delete"
        );
    }
}
