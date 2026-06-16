//! Per-PR enrichment persistence: submitted reviews, merged comments, and the reconciled CI
//! verdict, with the `pr_tests` row doubling as the "has this PR ever been enriched?" marker.

use rusqlite::{params, OptionalExtension};

use crate::error::StoreError;
use crate::store::Store;

impl Store {
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
