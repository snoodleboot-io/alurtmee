use crate::user::User;

/// The application's view of GitHub authentication.
///
/// Drives the settings UI: it decides whether the repo picker is shown and what validation
/// feedback the user sees. The token itself is **never** held here — it lives only in the OS
/// keychain (ARD AD-6); this state carries just the *outcome* of validating it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AuthState {
    /// No token has been validated yet.
    #[default]
    Unauthenticated,
    /// A token validated successfully against `GET /user`.
    Authenticated(User),
    /// A token was supplied but rejected (e.g. 401); carries a user-facing reason.
    Invalid(String),
}

impl AuthState {
    /// Whether we currently hold a validated identity.
    pub fn is_authenticated(&self) -> bool {
        matches!(self, AuthState::Authenticated(_))
    }

    /// The authenticated user, if any.
    pub fn user(&self) -> Option<&User> {
        match self {
            AuthState::Authenticated(user) => Some(user),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> User {
        User {
            id: 1,
            login: "octocat".to_string(),
        }
    }

    #[test]
    fn default_is_unauthenticated() {
        assert_eq!(AuthState::default(), AuthState::Unauthenticated);
    }

    #[test]
    fn is_authenticated_only_when_authenticated() {
        let cases = [
            (AuthState::Unauthenticated, false),
            (AuthState::Invalid("bad token".to_string()), false),
            (AuthState::Authenticated(user()), true),
        ];
        for (state, expected) in cases {
            assert_eq!(state.is_authenticated(), expected, "state: {state:?}");
        }
    }

    #[test]
    fn user_accessor_returns_some_only_when_authenticated() {
        assert_eq!(AuthState::Authenticated(user()).user(), Some(&user()));
        assert_eq!(AuthState::Unauthenticated.user(), None);
        assert_eq!(AuthState::Invalid("x".to_string()).user(), None);
    }
}
