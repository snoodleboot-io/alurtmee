use serde::Deserialize;

use super::wire_repo_owner::WireRepoOwner;

/// GitHub's repository payload (subset). The owner is nested; we flatten it on the way to
/// `domain::Repo`.
#[derive(Debug, Deserialize)]
pub(crate) struct WireRepo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub owner: WireRepoOwner,
}

impl From<WireRepo> for domain::Repo {
    fn from(w: WireRepo) -> Self {
        domain::Repo {
            id: w.id,
            owner: w.owner.login,
            name: w.name,
            full_name: w.full_name,
            private: w.private,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_repo_flattens_nested_owner() {
        let json = r#"{
            "id":42,
            "name":"hello",
            "full_name":"octocat/hello",
            "private":true,
            "owner":{"login":"octocat","id":1,"type":"User"},
            "description":"ignored extra"
        }"#;
        let wire: WireRepo = serde_json::from_str(json).unwrap();
        let repo: domain::Repo = wire.into();
        assert_eq!(repo.owner, "octocat");
        assert_eq!(repo.full_name, "octocat/hello");
        assert!(repo.private);
    }
}
