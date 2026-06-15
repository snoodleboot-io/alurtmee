use rusqlite::{params, Connection, OptionalExtension};

use crate::error::StoreError;
use crate::etag_record::EtagRecord;
use crate::migration;

/// Config key under which the persisted [`domain::RepoSelection`] JSON is stored.
const REPO_SELECTION_KEY: &str = "repo_selection";

/// Owns the SQLite connection for the application.
///
/// Backs non-secret configuration only — notably the repo selection. The GitHub token never
/// touches this store; it lives in the OS keychain (see [`crate::Keychain`], ARD AD-6).
pub struct Store {
    conn: Connection,
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

    /// Insert or overwrite a non-secret config value.
    pub fn set_config(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO config (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Read a config value, returning `None` if the key is absent.
    pub fn get_config(&self, key: &str) -> Result<Option<String>, StoreError> {
        let value = self
            .conn
            .query_row(
                "SELECT value FROM config WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(value)
    }

    /// Persist the repo selection as JSON under the `repo_selection` config key.
    pub fn save_selection(&self, selection: &domain::RepoSelection) -> Result<(), StoreError> {
        let json =
            serde_json::to_string(selection).map_err(|e| StoreError::Decode(e.to_string()))?;
        self.set_config(REPO_SELECTION_KEY, &json)
    }

    /// Load the repo selection, returning the empty default if none has been saved.
    pub fn load_selection(&self) -> Result<domain::RepoSelection, StoreError> {
        match self.get_config(REPO_SELECTION_KEY)? {
            None => Ok(domain::RepoSelection::default()),
            Some(json) => {
                serde_json::from_str(&json).map_err(|e| StoreError::Decode(e.to_string()))
            }
        }
    }

    /// Insert or overwrite the cached conditional-request validators for `endpoint`.
    ///
    /// Only non-secret response headers are stored — never a token (ARD AD-6).
    pub fn set_etag(&self, endpoint: &str, record: &EtagRecord) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO etags (endpoint, etag, last_modified) VALUES (?1, ?2, ?3)
             ON CONFLICT(endpoint) DO UPDATE SET
                 etag = excluded.etag,
                 last_modified = excluded.last_modified",
            params![endpoint, record.etag, record.last_modified],
        )?;
        Ok(())
    }

    /// Read the cached validators for `endpoint`, returning `None` if none are stored.
    pub fn get_etag(&self, endpoint: &str) -> Result<Option<EtagRecord>, StoreError> {
        let record = self
            .conn
            .query_row(
                "SELECT etag, last_modified FROM etags WHERE endpoint = ?1",
                params![endpoint],
                |row| {
                    Ok(EtagRecord {
                        etag: row.get(0)?,
                        last_modified: row.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(record)
    }

    /// Replace the full cached set of open PRs for `repo`.
    ///
    /// Runs in a single transaction: every existing row for `repo` is deleted, then `prs` are
    /// inserted. This gives REPLACE semantics scoped to the repo (other repos are untouched) and is
    /// atomic — a failed insert rolls the whole swap back. `draft` is persisted as a 0/1 integer.
    pub fn cache_repo_prs(
        &mut self,
        repo: &str,
        prs: &[domain::PullRequest],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM pull_requests WHERE repo = ?1", params![repo])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO pull_requests
                     (repo, number, title, author, draft, updated_at, url, head_sha)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for pr in prs {
                stmt.execute(params![
                    repo,
                    pr.id.number,
                    pr.title,
                    pr.author,
                    pr.draft as i64,
                    pr.updated_at,
                    pr.url,
                    pr.head_sha,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Load all cached open PRs for `repo`, ordered by PR number.
    ///
    /// Returns an empty `Vec` for a repo that has never been cached.
    pub fn load_repo_prs(&self, repo: &str) -> Result<Vec<domain::PullRequest>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT number, title, author, draft, updated_at, url, head_sha
             FROM pull_requests
             WHERE repo = ?1
             ORDER BY number",
        )?;
        let rows = stmt.query_map(params![repo], |row| {
            let number: u64 = row.get(0)?;
            let draft: i64 = row.get(3)?;
            Ok(domain::PullRequest {
                id: domain::PrId::new(repo, number),
                title: row.get(1)?,
                author: row.get(2)?,
                draft: draft != 0,
                updated_at: row.get(4)?,
                url: row.get(5)?,
                head_sha: row.get(6)?,
            })
        })?;

        let mut prs = Vec::new();
        for pr in rows {
            prs.push(pr?);
        }
        Ok(prs)
    }

    /// Atomically replace the stored enrichment for a single PR.
    ///
    /// Runs in one transaction keyed by `(e.id.repo, e.id.number)`: existing `reviews`, `comments`,
    /// and `pr_tests` rows for that PR are deleted, then the new reviews/comments are inserted with
    /// their list index (`idx`, preserving order on reload) and the `pr_tests` row is upserted. A
    /// failure rolls the whole swap back; enrichment for other PRs is untouched.
    ///
    /// Only non-secret PR data is written — never a token (ARD AD-6).
    pub fn save_enrichment(&mut self, e: &domain::PrEnrichment) -> Result<(), StoreError> {
        let repo = &e.id.repo;
        let number = e.id.number;

        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM reviews WHERE repo = ?1 AND number = ?2",
            params![repo, number],
        )?;
        tx.execute(
            "DELETE FROM comments WHERE repo = ?1 AND number = ?2",
            params![repo, number],
        )?;
        tx.execute(
            "DELETE FROM pr_tests WHERE repo = ?1 AND number = ?2",
            params![repo, number],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO reviews (repo, number, idx, author, state, submitted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (idx, review) in e.reviews.iter().enumerate() {
                stmt.execute(params![
                    repo,
                    number,
                    idx as i64,
                    review.author,
                    review.state,
                    review.submitted_at,
                ])?;
            }
        }
        {
            let mut stmt = tx.prepare(
                "INSERT INTO comments (repo, number, idx, kind, author, body, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (idx, comment) in e.comments.iter().enumerate() {
                stmt.execute(params![
                    repo,
                    number,
                    idx as i64,
                    comment_kind_to_text(comment.kind),
                    comment.author,
                    comment.body,
                    comment.created_at,
                ])?;
            }
        }
        tx.execute(
            "INSERT INTO pr_tests (repo, number, passed, failed, pending, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                repo,
                number,
                e.tests.passed,
                e.tests.failed,
                e.tests.pending,
                test_state_to_text(e.tests.state),
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Load the stored enrichment for `id`, or `None` if the PR was never enriched.
    ///
    /// The `pr_tests` row is the presence marker: a PR with no `pr_tests` row has never been
    /// enriched and yields `None` (distinct from an enriched PR that happens to have no reviews or
    /// comments). When present, reviews and comments are returned in their stored `idx` order.
    pub fn load_enrichment(
        &self,
        id: &domain::PrId,
    ) -> Result<Option<domain::PrEnrichment>, StoreError> {
        let repo = &id.repo;
        let number = id.number;

        let tests = self
            .conn
            .query_row(
                "SELECT passed, failed, pending, state FROM pr_tests
                 WHERE repo = ?1 AND number = ?2",
                params![repo, number],
                |row| {
                    let state: String = row.get(3)?;
                    Ok(domain::TestSummary {
                        passed: row.get(0)?,
                        failed: row.get(1)?,
                        pending: row.get(2)?,
                        state: test_state_from_text(&state),
                    })
                },
            )
            .optional()?;

        let tests = match tests {
            None => return Ok(None),
            Some(tests) => tests,
        };

        let mut review_stmt = self.conn.prepare(
            "SELECT author, state, submitted_at FROM reviews
             WHERE repo = ?1 AND number = ?2
             ORDER BY idx",
        )?;
        let review_rows = review_stmt.query_map(params![repo, number], |row| {
            Ok(domain::Review {
                author: row.get(0)?,
                state: row.get(1)?,
                submitted_at: row.get(2)?,
            })
        })?;
        let mut reviews = Vec::new();
        for review in review_rows {
            reviews.push(review?);
        }

        let mut comment_stmt = self.conn.prepare(
            "SELECT kind, author, body, created_at FROM comments
             WHERE repo = ?1 AND number = ?2
             ORDER BY idx",
        )?;
        let comment_rows = comment_stmt.query_map(params![repo, number], |row| {
            let kind: String = row.get(0)?;
            Ok(domain::Comment {
                author: row.get(1)?,
                kind: comment_kind_from_text(&kind),
                body: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        let mut comments = Vec::new();
        for comment in comment_rows {
            comments.push(comment?);
        }

        Ok(Some(domain::PrEnrichment::new(
            id.clone(),
            reviews,
            comments,
            tests,
        )))
    }
}

/// Map a [`domain::CommentKind`] to its stored text form.
fn comment_kind_to_text(kind: domain::CommentKind) -> &'static str {
    match kind {
        domain::CommentKind::Issue => "issue",
        domain::CommentKind::Review => "review",
    }
}

/// Map stored text back to a [`domain::CommentKind`]. An unknown value defaults to `Issue`
/// (defensive: we never write unknown values).
fn comment_kind_from_text(text: &str) -> domain::CommentKind {
    match text {
        "review" => domain::CommentKind::Review,
        _ => domain::CommentKind::Issue,
    }
}

/// Map a [`domain::TestState`] to its stored text form.
fn test_state_to_text(state: domain::TestState) -> &'static str {
    match state {
        domain::TestState::None => "none",
        domain::TestState::Pending => "pending",
        domain::TestState::Passing => "passing",
        domain::TestState::Failing => "failing",
    }
}

/// Map stored text back to a [`domain::TestState`]. An unknown value defaults to `None`
/// (defensive: we never write unknown values).
fn test_state_from_text(text: &str) -> domain::TestState {
    match text {
        "pending" => domain::TestState::Pending,
        "passing" => domain::TestState::Passing,
        "failing" => domain::TestState::Failing,
        _ => domain::TestState::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(version, 3);
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
}
