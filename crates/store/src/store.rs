use rusqlite::Connection;

use crate::error::StoreError;
use crate::migration;

/// Owns the SQLite connection for the application.
///
/// The connection is the single owned resource; the data operations live in focused sibling modules
/// ([`store_config`](crate::store_config), [`store_cache`](crate::store_cache),
/// [`store_enrichment`](crate::store_enrichment), [`store_classifier`](crate::store_classifier),
/// [`store_ci`](crate::store_ci)) as separate `impl Store` blocks, so each concern is isolated while
/// sharing one connection. Backs non-secret configuration only — the GitHub token never touches this
/// store; it lives in the OS keychain (see [`crate::Keychain`], ARD AD-6).
pub struct Store {
    /// Shared with the focused `impl Store` modules in this crate; not part of the public API.
    pub(crate) conn: Connection,
}

impl Store {
    /// Open a file-backed database at `path`, creating it if absent, and run migrations.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let store = Self {
            conn: Connection::open(path)?,
        };
        migration::migrate(&store.conn)?;
        Ok(store)
    }

    /// Open an ephemeral in-memory database (used by tests and one-shot tooling) and migrate it.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let store = Self {
            conn: Connection::open_in_memory()?,
        };
        migration::migrate(&store.conn)?;
        Ok(store)
    }

    /// Borrow the underlying connection for ad-hoc schema/query work.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::etag_record::EtagRecord;
    use rusqlite::{params, OptionalExtension};

    #[test]
    fn open_in_memory_executes_trivial_query() {
        let store = Store::open_in_memory().expect("open in-memory database");
        let value: i64 = store
            .connection()
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("run SELECT 1");
        assert_eq!(value, 1);
    }

    #[test]
    fn open_creates_file_backed_database() {
        let mut path = std::env::temp_dir();
        path.push(format!("alurtmee_store_test_{}.sqlite", std::process::id()));
        let path_str = path.to_str().expect("utf-8 temp path");

        let store = Store::open(path_str).expect("open file-backed database");
        let value: i64 = store
            .connection()
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("run SELECT 1");
        assert_eq!(value, 1);

        drop(store);
        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn migration_creates_config_table_and_sets_version() {
        let store = Store::open_in_memory().expect("open + migrate");

        let table: Option<String> = store
            .connection()
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'config'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query sqlite_master");
        assert_eq!(table.as_deref(), Some("config"));

        let version: i64 = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, migration::SCHEMA_VERSION);
    }

    #[test]
    fn migration_is_idempotent() {
        let store = Store::open_in_memory().expect("open + migrate");

        // Re-run migration directly; version must stay current and the tables must survive.
        migration::migrate(store.connection()).expect("second migrate");
        migration::migrate(store.connection()).expect("third migrate");

        let version: i64 = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, migration::SCHEMA_VERSION);
    }

    #[test]
    fn config_round_trip_and_absent_key() {
        let store = Store::open_in_memory().expect("open store");

        assert_eq!(store.get_config("missing").expect("get absent"), None);

        store.set_config("theme", "dark").expect("set");
        assert_eq!(
            store.get_config("theme").expect("get present"),
            Some("dark".to_string())
        );

        // UPSERT overwrites.
        store.set_config("theme", "light").expect("update");
        assert_eq!(
            store.get_config("theme").expect("get updated"),
            Some("light".to_string())
        );
    }

    #[test]
    fn selection_round_trip() {
        let store = Store::open_in_memory().expect("open store");

        // Fresh store yields the empty default.
        assert!(store.load_selection().expect("load default").is_empty());

        let selection: domain::RepoSelection = ["octocat/hello", "rust-lang/rust"]
            .into_iter()
            .map(String::from)
            .collect();
        store.save_selection(&selection).expect("save");

        let loaded = store.load_selection().expect("load saved");
        assert_eq!(loaded, selection);
    }

    #[test]
    fn load_selection_rejects_malformed_json() {
        let store = Store::open_in_memory().expect("open store");
        store
            .set_config("repo_selection", "{ not valid json")
            .expect("seed bad json");

        let err = store.load_selection().expect_err("decode should fail");
        assert!(matches!(err, StoreError::Decode(_)));
    }

    fn sample_pr(repo: &str, number: u64, draft: bool) -> domain::PullRequest {
        domain::PullRequest {
            id: domain::PrId::new(repo, number),
            title: format!("PR {number}"),
            author: "octocat".to_string(),
            draft,
            updated_at: "2026-06-14T00:00:00Z".to_string(),
            url: format!("https://github.com/{repo}/pull/{number}"),
            head_sha: String::new(),
            author_type: String::new(),
            head_ref: String::new(),
            labels: Vec::new(),
        }
    }

    #[test]
    fn etag_round_trip_with_and_without_last_modified() {
        let store = Store::open_in_memory().expect("open store");

        let full = EtagRecord {
            etag: Some("\"abc123\"".to_string()),
            last_modified: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
        };
        store
            .set_etag("/repos/octocat/hello/pulls", &full)
            .expect("set full");
        assert_eq!(
            store
                .get_etag("/repos/octocat/hello/pulls")
                .expect("get full"),
            Some(full)
        );

        let etag_only = EtagRecord {
            etag: Some("\"def456\"".to_string()),
            last_modified: None,
        };
        store
            .set_etag("/repos/foo/bar/pulls", &etag_only)
            .expect("set etag-only");
        assert_eq!(
            store
                .get_etag("/repos/foo/bar/pulls")
                .expect("get etag-only"),
            Some(etag_only)
        );
    }

    #[test]
    fn get_etag_absent_endpoint_is_none() {
        let store = Store::open_in_memory().expect("open store");
        assert_eq!(store.get_etag("/never/fetched").expect("get absent"), None);
    }

    #[test]
    fn set_etag_upsert_overwrites() {
        let store = Store::open_in_memory().expect("open store");

        store
            .set_etag(
                "/endpoint",
                &EtagRecord {
                    etag: Some("\"v1\"".to_string()),
                    last_modified: None,
                },
            )
            .expect("first set");
        store
            .set_etag(
                "/endpoint",
                &EtagRecord {
                    etag: Some("\"v2\"".to_string()),
                    last_modified: Some("later".to_string()),
                },
            )
            .expect("overwrite");

        assert_eq!(
            store.get_etag("/endpoint").expect("get overwritten"),
            Some(EtagRecord {
                etag: Some("\"v2\"".to_string()),
                last_modified: Some("later".to_string()),
            })
        );
    }

    #[test]
    fn cache_and_load_repo_prs_round_trip_ordered() {
        let mut store = Store::open_in_memory().expect("open store");

        // Insert out of order to prove load orders by number.
        let prs = vec![
            sample_pr("octocat/hello", 7, false),
            sample_pr("octocat/hello", 2, true),
        ];
        store.cache_repo_prs("octocat/hello", &prs).expect("cache");

        let loaded = store.load_repo_prs("octocat/hello").expect("load");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id.number, 2);
        assert_eq!(loaded[1].id.number, 7);
        assert_eq!(loaded[0], sample_pr("octocat/hello", 2, true));
        assert_eq!(loaded[1], sample_pr("octocat/hello", 7, false));
    }

    #[test]
    fn cache_repo_prs_replaces_set_for_repo_only() {
        let mut store = Store::open_in_memory().expect("open store");

        store
            .cache_repo_prs(
                "octocat/hello",
                &[
                    sample_pr("octocat/hello", 1, false),
                    sample_pr("octocat/hello", 2, false),
                ],
            )
            .expect("cache hello v1");
        store
            .cache_repo_prs("other/repo", &[sample_pr("other/repo", 9, false)])
            .expect("cache other");

        // Re-cache hello with a wholly different set: old rows for hello must be gone.
        store
            .cache_repo_prs("octocat/hello", &[sample_pr("octocat/hello", 5, true)])
            .expect("cache hello v2");

        let hello = store.load_repo_prs("octocat/hello").expect("load hello");
        assert_eq!(hello, vec![sample_pr("octocat/hello", 5, true)]);

        // Other repo untouched.
        let other = store.load_repo_prs("other/repo").expect("load other");
        assert_eq!(other, vec![sample_pr("other/repo", 9, false)]);
    }

    #[test]
    fn load_repo_prs_uncached_repo_is_empty() {
        let store = Store::open_in_memory().expect("open store");
        assert!(store
            .load_repo_prs("never/cached")
            .expect("load empty")
            .is_empty());
    }

    #[test]
    fn migration_creates_all_enrichment_tables() {
        let store = Store::open_in_memory().expect("open + migrate");

        for table in ["etags", "pull_requests", "reviews", "comments", "pr_tests"] {
            let found: Option<String> = store
                .connection()
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    params![table],
                    |row| row.get(0),
                )
                .optional()
                .expect("query sqlite_master");
            assert_eq!(found.as_deref(), Some(table));
        }

        let version: i64 = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, migration::SCHEMA_VERSION);
    }

    fn sample_enrichment(repo: &str, number: u64) -> domain::PrEnrichment {
        domain::PrEnrichment::new(
            domain::PrId::new(repo, number),
            vec![
                domain::Review {
                    author: "alice".to_string(),
                    state: "APPROVED".to_string(),
                    submitted_at: "2026-06-14T01:00:00Z".to_string(),
                },
                domain::Review {
                    author: "bob".to_string(),
                    state: "CHANGES_REQUESTED".to_string(),
                    submitted_at: "2026-06-14T02:00:00Z".to_string(),
                },
            ],
            vec![
                domain::Comment {
                    author: "carol".to_string(),
                    kind: domain::CommentKind::Issue,
                    body: "Looks good overall.".to_string(),
                    created_at: "2026-06-14T03:00:00Z".to_string(),
                },
                domain::Comment {
                    author: "dave".to_string(),
                    kind: domain::CommentKind::Review,
                    body: "Nit on line 42.".to_string(),
                    created_at: "2026-06-14T04:00:00Z".to_string(),
                },
            ],
            domain::TestSummary {
                passed: 2,
                failed: 1,
                pending: 0,
                state: domain::TestState::Failing,
            },
        )
    }

    #[test]
    fn enrichment_round_trip_preserves_order_and_kinds() {
        let mut store = Store::open_in_memory().expect("open store");
        let enrichment = sample_enrichment("octocat/hello", 7);

        store.save_enrichment(&enrichment).expect("save");
        let loaded = store
            .load_enrichment(&domain::PrId::new("octocat/hello", 7))
            .expect("load")
            .expect("present");

        assert_eq!(loaded, enrichment);

        // Spell out the order/kind/state guarantees the equality above subsumes.
        assert_eq!(loaded.reviews[0].author, "alice");
        assert_eq!(loaded.reviews[1].author, "bob");
        assert_eq!(loaded.comments[0].kind, domain::CommentKind::Issue);
        assert_eq!(loaded.comments[1].kind, domain::CommentKind::Review);
        assert_eq!(loaded.tests.state, domain::TestState::Failing);
        assert_eq!(loaded.tests.passed, 2);
        assert_eq!(loaded.tests.failed, 1);
    }

    #[test]
    fn load_enrichment_never_enriched_is_none() {
        let store = Store::open_in_memory().expect("open store");
        assert!(store
            .load_enrichment(&domain::PrId::new("never/enriched", 1))
            .expect("load")
            .is_none());
    }

    #[test]
    fn save_enrichment_replaces_set_for_pr_only() {
        let mut store = Store::open_in_memory().expect("open store");

        store
            .save_enrichment(&sample_enrichment("octocat/hello", 7))
            .expect("save hello v1");
        let other = sample_enrichment("other/repo", 3);
        store.save_enrichment(&other).expect("save other");

        // Re-save hello with a wholly different, smaller set.
        let replacement = domain::PrEnrichment::new(
            domain::PrId::new("octocat/hello", 7),
            vec![domain::Review {
                author: "eve".to_string(),
                state: "COMMENTED".to_string(),
                submitted_at: "2026-06-14T05:00:00Z".to_string(),
            }],
            Vec::new(),
            domain::TestSummary {
                passed: 1,
                failed: 0,
                pending: 0,
                state: domain::TestState::Passing,
            },
        );
        store.save_enrichment(&replacement).expect("save hello v2");

        let hello = store
            .load_enrichment(&domain::PrId::new("octocat/hello", 7))
            .expect("load hello")
            .expect("present");
        assert_eq!(hello, replacement);
        assert_eq!(hello.reviews.len(), 1);
        assert!(hello.comments.is_empty());
        assert_eq!(hello.tests.state, domain::TestState::Passing);

        // The other PR's enrichment is untouched.
        let other_loaded = store
            .load_enrichment(&domain::PrId::new("other/repo", 3))
            .expect("load other")
            .expect("present");
        assert_eq!(other_loaded, other);
    }

    #[test]
    fn enrichment_kind_and_state_text_mapping_round_trips() {
        let mut store = Store::open_in_memory().expect("open store");

        // Exercise every TestState and both CommentKinds through the persistence layer.
        for (number, state) in [
            (1, domain::TestState::None),
            (2, domain::TestState::Pending),
            (3, domain::TestState::Passing),
            (4, domain::TestState::Failing),
        ] {
            let enrichment = domain::PrEnrichment::new(
                domain::PrId::new("octocat/hello", number),
                Vec::new(),
                vec![
                    domain::Comment {
                        author: "a".to_string(),
                        kind: domain::CommentKind::Issue,
                        body: "issue".to_string(),
                        created_at: "t".to_string(),
                    },
                    domain::Comment {
                        author: "b".to_string(),
                        kind: domain::CommentKind::Review,
                        body: "review".to_string(),
                        created_at: "t".to_string(),
                    },
                ],
                domain::TestSummary {
                    passed: 0,
                    failed: 0,
                    pending: 0,
                    state,
                },
            );
            store.save_enrichment(&enrichment).expect("save");

            let loaded = store
                .load_enrichment(&domain::PrId::new("octocat/hello", number))
                .expect("load")
                .expect("present");
            assert_eq!(loaded.tests.state, state);
            assert_eq!(loaded.comments[0].kind, domain::CommentKind::Issue);
            assert_eq!(loaded.comments[1].kind, domain::CommentKind::Review);
        }

        // Verify the stored text forms directly.
        let kind: String = store
            .connection()
            .query_row(
                "SELECT kind FROM comments WHERE repo = 'octocat/hello' AND number = 1 AND idx = 0",
                [],
                |row| row.get(0),
            )
            .expect("read kind text");
        assert_eq!(kind, "issue");
        let state_text: String = store
            .connection()
            .query_row(
                "SELECT state FROM pr_tests WHERE repo = 'octocat/hello' AND number = 4",
                [],
                |row| row.get(0),
            )
            .expect("read state text");
        assert_eq!(state_text, "failing");
    }

    #[test]
    fn migration_creates_classifier_config_tables() {
        let store = Store::open_in_memory().expect("open + migrate");

        for table in ["corrections", "repo_classifier_config"] {
            let found: Option<String> = store
                .connection()
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    params![table],
                    |row| row.get(0),
                )
                .optional()
                .expect("query sqlite_master");
            assert_eq!(found.as_deref(), Some(table));
        }

        let version: i64 = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, migration::SCHEMA_VERSION);
    }

    #[test]
    fn correction_round_trip_for_each_kind() {
        let store = Store::open_in_memory().expect("open store");

        for (number, kind) in [
            (1, domain::CategoryKind::Feature),
            (2, domain::CategoryKind::Security),
            (3, domain::CategoryKind::Unknown),
        ] {
            store
                .set_correction("octocat/hello", number, kind)
                .expect("set correction");
            assert_eq!(
                store
                    .get_correction("octocat/hello", number)
                    .expect("get correction"),
                Some(kind)
            );
        }

        // The stored text form is the bare serde value (no surrounding object).
        let text: String = store
            .connection()
            .query_row(
                "SELECT category FROM corrections WHERE repo = 'octocat/hello' AND number = 1",
                [],
                |row| row.get(0),
            )
            .expect("read category text");
        assert_eq!(text, "\"feature\"");
    }

    #[test]
    fn set_correction_upsert_overwrites() {
        let store = Store::open_in_memory().expect("open store");

        store
            .set_correction("octocat/hello", 7, domain::CategoryKind::Feature)
            .expect("set");
        store
            .set_correction("octocat/hello", 7, domain::CategoryKind::Security)
            .expect("overwrite");

        assert_eq!(
            store.get_correction("octocat/hello", 7).expect("get"),
            Some(domain::CategoryKind::Security)
        );
    }

    #[test]
    fn get_correction_absent_is_none() {
        let store = Store::open_in_memory().expect("open store");
        assert_eq!(
            store.get_correction("never/corrected", 1).expect("get"),
            None
        );
    }

    #[test]
    fn get_correction_rejects_malformed_value() {
        let store = Store::open_in_memory().expect("open store");
        store
            .connection()
            .execute(
                "INSERT INTO corrections (repo, number, category)
                 VALUES ('octocat/hello', 9, 'not-a-category')",
                [],
            )
            .expect("seed bad value");

        let err = store
            .get_correction("octocat/hello", 9)
            .expect_err("decode should fail");
        assert!(matches!(err, StoreError::Decode(_)));
    }

    #[test]
    fn clear_correction_removes_and_is_noop_when_absent() {
        let store = Store::open_in_memory().expect("open store");

        // Clearing an absent PR is a no-op and returns Ok.
        store
            .clear_correction("octocat/hello", 7)
            .expect("clear absent is ok");

        store
            .set_correction("octocat/hello", 7, domain::CategoryKind::Security)
            .expect("set");
        store
            .clear_correction("octocat/hello", 7)
            .expect("clear present");
        assert_eq!(store.get_correction("octocat/hello", 7).expect("get"), None);
    }

    #[test]
    fn label_map_round_trip_and_absent_is_none() {
        let store = Store::open_in_memory().expect("open store");

        assert_eq!(
            store
                .load_label_map("never/configured")
                .expect("load absent"),
            None
        );

        let map = domain::LabelMap::with_common_defaults();
        store.save_label_map("octocat/hello", &map).expect("save");
        assert_eq!(
            store.load_label_map("octocat/hello").expect("load"),
            Some(map)
        );
    }

    #[test]
    fn load_label_map_rejects_malformed_json() {
        let store = Store::open_in_memory().expect("open store");
        store
            .connection()
            .execute(
                "INSERT INTO repo_classifier_config (repo, label_map_json)
                 VALUES ('octocat/hello', '{ not valid json')",
                [],
            )
            .expect("seed bad json");

        let err = store
            .load_label_map("octocat/hello")
            .expect_err("decode should fail");
        assert!(matches!(err, StoreError::Decode(_)));
    }

    #[test]
    fn bot_overrides_round_trip_and_absent_is_none() {
        let store = Store::open_in_memory().expect("open store");

        assert_eq!(
            store
                .load_bot_overrides("never/configured")
                .expect("load absent"),
            None
        );

        let mut overrides = domain::BotOverrides::new();
        overrides.force_bot("x").force_human("y");
        store
            .save_bot_overrides("octocat/hello", &overrides)
            .expect("save");
        assert_eq!(
            store.load_bot_overrides("octocat/hello").expect("load"),
            Some(overrides)
        );
    }

    #[test]
    fn label_map_and_bot_overrides_are_independent() {
        let store = Store::open_in_memory().expect("open store");

        let map = domain::LabelMap::with_common_defaults();
        store
            .save_label_map("octocat/hello", &map)
            .expect("save map");

        let mut overrides = domain::BotOverrides::new();
        overrides.force_bot("dependabot[bot]");
        store
            .save_bot_overrides("octocat/hello", &overrides)
            .expect("save overrides");

        // Both upserts touched a single column each, so neither clobbered the other.
        assert_eq!(
            store.load_label_map("octocat/hello").expect("load map"),
            Some(map)
        );
        assert_eq!(
            store
                .load_bot_overrides("octocat/hello")
                .expect("load overrides"),
            Some(overrides)
        );

        // Order-independence: saving overrides first, then a map, also preserves both.
        let mut overrides2 = domain::BotOverrides::new();
        overrides2.force_human("real-person");
        store
            .save_bot_overrides("other/repo", &overrides2)
            .expect("save overrides first");
        let mut map2 = domain::LabelMap::new();
        map2.insert("regression", domain::CategoryKind::Security);
        store
            .save_label_map("other/repo", &map2)
            .expect("save map second");

        assert_eq!(
            store.load_bot_overrides("other/repo").expect("load"),
            Some(overrides2)
        );
        assert_eq!(
            store.load_label_map("other/repo").expect("load"),
            Some(map2)
        );
    }

    fn sample_run(
        repo: &str,
        id: u64,
        workflow: &str,
        conclusion: Option<&str>,
        duration_secs: u64,
    ) -> domain::WorkflowRun {
        domain::WorkflowRun {
            id,
            repo: repo.to_string(),
            workflow: workflow.to_string(),
            conclusion: conclusion.map(String::from),
            duration_secs,
        }
    }

    #[test]
    fn migration_creates_ci_runs_table() {
        let store = Store::open_in_memory().expect("open + migrate");

        let found: Option<String> = store
            .connection()
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'ci_runs'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query sqlite_master");
        assert_eq!(found.as_deref(), Some("ci_runs"));

        let version: i64 = store
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, migration::SCHEMA_VERSION);
    }

    #[test]
    fn record_run_returns_true_first_then_false_for_duplicate() {
        let store = Store::open_in_memory().expect("open store");

        let run = sample_run("octocat/hello", 100, "CI", Some("success"), 42);
        assert!(store.record_run(&run).expect("first record"));
        // Same (repo, run_id) → no new row inserted.
        assert!(!store.record_run(&run).expect("second record"));
    }

    #[test]
    fn recent_durations_returns_completed_matching_runs_newest_first() {
        let store = Store::open_in_memory().expect("open store");

        // Completed runs for (repo, "CI") with increasing run_id.
        store
            .record_run(&sample_run("octocat/hello", 1, "CI", Some("success"), 10))
            .expect("record");
        store
            .record_run(&sample_run("octocat/hello", 2, "CI", Some("failure"), 20))
            .expect("record");
        store
            .record_run(&sample_run("octocat/hello", 3, "CI", Some("success"), 30))
            .expect("record");
        // A different workflow for the same repo — must be excluded.
        store
            .record_run(&sample_run(
                "octocat/hello",
                4,
                "Deploy",
                Some("success"),
                99,
            ))
            .expect("record");
        // A different repo, same workflow — must be excluded.
        store
            .record_run(&sample_run("other/repo", 5, "CI", Some("success"), 88))
            .expect("record");
        // An in-progress run (no conclusion) for the target — must be excluded.
        store
            .record_run(&sample_run("octocat/hello", 6, "CI", None, 77))
            .expect("record");

        // No limit pressure: only the three completed (repo, "CI") durations, newest-first.
        let all = store
            .recent_durations("octocat/hello", "CI", 10)
            .expect("recent");
        assert_eq!(all, vec![30, 20, 10]);

        // limit caps the result, still newest-first.
        let capped = store
            .recent_durations("octocat/hello", "CI", 2)
            .expect("recent capped");
        assert_eq!(capped, vec![30, 20]);
    }

    #[test]
    fn recent_durations_unknown_pair_is_empty() {
        let store = Store::open_in_memory().expect("open store");
        assert!(store
            .recent_durations("never/recorded", "CI", 10)
            .expect("recent")
            .is_empty());
    }
}
