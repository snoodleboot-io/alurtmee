use serde::Deserialize;

/// GitHub's `GET /user` payload (subset). Extra fields (`type`, `name`, ...) are ignored.
#[derive(Debug, Deserialize)]
pub(crate) struct WireUser {
    pub id: u64,
    pub login: String,
}

impl From<WireUser> for domain::User {
    fn from(w: WireUser) -> Self {
        domain::User {
            id: w.id,
            login: w.login,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_user_ignores_extra_fields_and_maps_to_domain() {
        let json = r#"{"id":1,"login":"octocat","type":"User","name":"The Octocat"}"#;
        let wire: WireUser = serde_json::from_str(json).unwrap();
        let user: domain::User = wire.into();
        assert_eq!(
            user,
            domain::User {
                id: 1,
                login: "octocat".to_string()
            }
        );
    }
}
