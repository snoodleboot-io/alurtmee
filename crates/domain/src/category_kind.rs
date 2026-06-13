use serde::{Deserialize, Serialize};

/// Feature-vs-security classification outcome (AD-5).
///
/// `Unknown` is a first-class value: when no layer fires confidently we record the uncertainty
/// rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryKind {
    Feature,
    Security,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_kind_serializes_to_snake_case() {
        let cases = [
            (CategoryKind::Feature, "\"feature\""),
            (CategoryKind::Security, "\"security\""),
            (CategoryKind::Unknown, "\"unknown\""),
        ];
        for (kind, expected) in cases {
            let json = serde_json::to_string(&kind).expect("serialize CategoryKind");
            assert_eq!(json, expected);
        }
    }
}
