//! The layered feature-vs-security classifier (AD-5).
//!
//! **Why layered, first-confident-wins, with the firing signal recorded (§3.6):** the signals are
//! ordered by *authority*, not strength. A human-applied **label** is a deliberate statement of
//! intent, so it outranks every heuristic; a **title/branch prefix** is a weaker but still
//! author-chosen convention; **changed paths** and **Dependabot** are inferences we make *for* the
//! author. Trying to blend them into one opaque score would make a wrong verdict impossible to
//! explain or trust. Instead we take the first confident layer and record *which* one fired
//! ([`Category::signal`]), so every verdict is auditable and a user correction (which short-circuits
//! the whole chain) never has to fight a heuristic.

use crate::category::Category;
use crate::category_kind::CategoryKind;
use crate::classification_input::ClassificationInput;
use crate::label_map::LabelMap;

/// Lockfile basenames that, when changed, are a dependency/security-relevant signal.
const LOCKFILES: &[&str] = &[
    "cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "go.sum",
    "poetry.lock",
    "gemfile.lock",
];

/// Substrings within a changed path that mark it security-sensitive.
const SENSITIVE_PATH_MARKERS: &[&str] = &["auth", "crypto", "security", "secret"];

/// Classify a PR into [`CategoryKind`], recording the firing signal and a confidence.
///
/// A user `correction` short-circuits everything (confidence `1.0`, signal `"correction"`).
/// Otherwise the layers are tried in precedence order — labels → title/branch prefix → changed
/// paths → Dependabot — and the first confident one wins; if none fire, the result is
/// [`CategoryKind::Unknown`] (we record uncertainty rather than guess).
pub fn classify_category(
    input: &ClassificationInput,
    label_map: &LabelMap,
    correction: Option<CategoryKind>,
) -> Category {
    if let Some(kind) = correction {
        return Category {
            kind,
            confidence: 1.0,
            signal: "correction".to_string(),
        };
    }

    signal_labels(input.labels, label_map)
        .or_else(|| signal_prefix(input.title, input.head_ref))
        .or_else(|| signal_paths(input.changed_paths))
        .or_else(|| signal_dependabot(input.author_login))
        .unwrap_or_else(|| Category {
            kind: CategoryKind::Unknown,
            confidence: 0.0,
            signal: "none".to_string(),
        })
}

/// Layer 1 — a label mapped in the per-repo [`LabelMap`] (highest authority).
fn signal_labels(labels: &[String], label_map: &LabelMap) -> Option<Category> {
    for label in labels {
        if let Some(kind) = label_map.get(label) {
            return Some(Category {
                kind,
                confidence: 0.95,
                signal: format!("label:{label}"),
            });
        }
    }
    None
}

/// Layer 2 — a security/feature convention in the title or branch name.
fn signal_prefix(title: &str, head_ref: &str) -> Option<Category> {
    let title = title.to_ascii_lowercase();
    let head_ref = head_ref.to_ascii_lowercase();

    let security = head_ref.starts_with("security/")
        || head_ref.starts_with("sec/")
        || title.starts_with("fix(sec")
        || title.starts_with("sec:")
        || title.contains("security")
        || title.contains("vulnerab")
        || title.contains("cve-");
    if security {
        return Some(Category {
            kind: CategoryKind::Security,
            confidence: 0.8,
            signal: "prefix:security".to_string(),
        });
    }

    let feature = title.starts_with("feat:")
        || title.starts_with("feat(")
        || head_ref.starts_with("feat/")
        || head_ref.starts_with("feature/");
    if feature {
        return Some(Category {
            kind: CategoryKind::Feature,
            confidence: 0.8,
            signal: "prefix:feature".to_string(),
        });
    }

    None
}

/// Layer 3 — a changed path touching a security-sensitive area or a dependency lockfile.
fn signal_paths(changed_paths: &[String]) -> Option<Category> {
    if changed_paths.iter().any(|p| is_sensitive_path(p)) {
        return Some(Category {
            kind: CategoryKind::Security,
            confidence: 0.7,
            signal: "path".to_string(),
        });
    }
    None
}

/// Layer 4 — an automated dependency PR from Dependabot (advisory-driven → security).
fn signal_dependabot(author_login: &str) -> Option<Category> {
    if author_login.starts_with("dependabot") {
        return Some(Category {
            kind: CategoryKind::Security,
            confidence: 0.9,
            signal: "dependabot".to_string(),
        });
    }
    None
}

/// Whether a changed file path is security-sensitive: a sensitive marker substring, a CI workflow
/// file, or a dependency lockfile.
fn is_sensitive_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    if path.starts_with(".github/workflows/") {
        return true;
    }
    if LOCKFILES.iter().any(|lock| path.ends_with(lock)) {
        return true;
    }
    SENSITIVE_PATH_MARKERS
        .iter()
        .any(|marker| path.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        author_login: &'a str,
        title: &'a str,
        head_ref: &'a str,
        labels: &'a [String],
        changed_paths: &'a [String],
    ) -> ClassificationInput<'a> {
        ClassificationInput {
            author_login,
            title,
            head_ref,
            labels,
            changed_paths,
        }
    }

    fn strs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn correction_short_circuits_all_signals() {
        let labels = strs(&["security"]);
        let paths = strs(&["src/auth/login.rs"]);
        let inp = input(
            "dependabot[bot]",
            "fix: security hole",
            "security/x",
            &labels,
            &paths,
        );
        let result = classify_category(
            &inp,
            &LabelMap::with_common_defaults(),
            Some(CategoryKind::Feature),
        );
        assert_eq!(result.kind, CategoryKind::Feature);
        assert_eq!(result.signal, "correction");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn label_outranks_prefix_path_and_dependabot() {
        // Everything points to Security, but a "feature" label must win (label has top authority).
        let labels = strs(&["feature"]);
        let paths = strs(&["src/auth/mod.rs"]);
        let inp = input(
            "dependabot[bot]",
            "security: patch",
            "security/x",
            &labels,
            &paths,
        );
        let result = classify_category(&inp, &LabelMap::with_common_defaults(), None);
        assert_eq!(result.kind, CategoryKind::Feature);
        assert!(result.signal.starts_with("label:"));
    }

    #[test]
    fn prefix_outranks_path_and_dependabot() {
        let labels: Vec<String> = vec![];
        let paths = strs(&["src/auth/mod.rs"]); // would be a path-security signal
        let inp = input(
            "dependabot[bot]",
            "feat: add widget",
            "feat/widget",
            &labels,
            &paths,
        );
        let result = classify_category(&inp, &LabelMap::with_common_defaults(), None);
        assert_eq!(result.kind, CategoryKind::Feature);
        assert_eq!(result.signal, "prefix:feature");
    }

    #[test]
    fn security_prefix_via_branch_and_title() {
        let labels: Vec<String> = vec![];
        let none: Vec<String> = vec![];
        for (title, head_ref) in [
            ("fix(sec): xss", "patch-1"),
            ("Patch CVE-2026-0001", "patch-1"),
            ("normal title", "security/fix"),
        ] {
            let inp = input("octocat", title, head_ref, &labels, &none);
            let result = classify_category(&inp, &LabelMap::new(), None);
            assert_eq!(result.kind, CategoryKind::Security, "{title} / {head_ref}");
            assert_eq!(result.signal, "prefix:security");
        }
    }

    #[test]
    fn path_signal_fires_for_sensitive_files_and_lockfiles() {
        let labels: Vec<String> = vec![];
        for path in [
            "src/crypto/aes.rs",
            "Cargo.lock",
            ".github/workflows/ci.yml",
            "auth/session.go",
        ] {
            let paths = strs(&[path]);
            let inp = input("octocat", "chore: bump", "chore/x", &labels, &paths);
            let result = classify_category(&inp, &LabelMap::new(), None);
            assert_eq!(result.kind, CategoryKind::Security, "path {path}");
            assert_eq!(result.signal, "path");
        }
    }

    #[test]
    fn dependabot_author_is_security_when_no_higher_signal() {
        let labels: Vec<String> = vec![];
        let paths = strs(&["README.md"]);
        let inp = input(
            "dependabot[bot]",
            "Bump serde from 1.0 to 1.1",
            "dependabot/serde",
            &labels,
            &paths,
        );
        let result = classify_category(&inp, &LabelMap::new(), None);
        assert_eq!(result.kind, CategoryKind::Security);
        assert_eq!(result.signal, "dependabot");
    }

    #[test]
    fn no_signal_is_unknown() {
        let labels: Vec<String> = vec![];
        let paths = strs(&["docs/readme.md"]);
        let inp = input("octocat", "Update docs", "docs/update", &labels, &paths);
        let result = classify_category(&inp, &LabelMap::with_common_defaults(), None);
        assert_eq!(result.kind, CategoryKind::Unknown);
        assert_eq!(result.signal, "none");
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn unmapped_label_falls_through_to_lower_signals() {
        let labels = strs(&["wontfix"]); // not in the map
        let none: Vec<String> = vec![];
        let inp = input("octocat", "feat: thing", "feat/thing", &labels, &none);
        let result = classify_category(&inp, &LabelMap::with_common_defaults(), None);
        assert_eq!(result.kind, CategoryKind::Feature);
        assert_eq!(result.signal, "prefix:feature");
    }
}
