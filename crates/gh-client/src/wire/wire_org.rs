use serde::Deserialize;

/// GitHub's organization payload from `GET /user/orgs` (subset).
#[derive(Debug, Deserialize)]
pub(crate) struct WireOrg {
    pub id: u64,
    pub login: String,
}

impl From<WireOrg> for domain::Org {
    fn from(w: WireOrg) -> Self {
        domain::Org {
            id: w.id,
            login: w.login,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_org_maps_to_domain() {
        let json = r#"{"id":100,"login":"acme","description":"ignored"}"#;
        let wire: WireOrg = serde_json::from_str(json).unwrap();
        let org: domain::Org = wire.into();
        assert_eq!(
            org,
            domain::Org {
                id: 100,
                login: "acme".to_string()
            }
        );
    }
}
