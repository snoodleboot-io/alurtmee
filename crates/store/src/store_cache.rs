//! The HTTP conditional-request cache (`etags`) and the cached open-PR snapshot per repo.

use rusqlite::{params, OptionalExtension};

use crate::error::StoreError;
use crate::etag_record::EtagRecord;
use crate::store::Store;

impl Store {
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
                     (repo, number, title, author, draft, updated_at, url, head_sha,
                      author_type, head_ref, labels_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for pr in prs {
                let labels_json = serde_json::to_string(&pr.labels)
                    .map_err(|e| StoreError::Decode(e.to_string()))?;
                stmt.execute(params![
                    repo,
                    pr.id.number,
                    pr.title,
                    pr.author,
                    pr.draft as i64,
                    pr.updated_at,
                    pr.url,
                    pr.head_sha,
                    pr.author_type,
                    pr.head_ref,
                    labels_json,
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
            "SELECT number, title, author, draft, updated_at, url, head_sha,
                    author_type, head_ref, labels_json
             FROM pull_requests
             WHERE repo = ?1
             ORDER BY number",
        )?;
        let rows = stmt.query_map(params![repo], |row| {
            let number: u64 = row.get(0)?;
            let draft: i64 = row.get(3)?;
            let labels_json: String = row.get(9)?;
            Ok(domain::PullRequest {
                id: domain::PrId::new(repo, number),
                title: row.get(1)?,
                author: row.get(2)?,
                draft: draft != 0,
                updated_at: row.get(4)?,
                url: row.get(5)?,
                head_sha: row.get(6)?,
                author_type: row.get(7)?,
                head_ref: row.get(8)?,
                labels: serde_json::from_str(&labels_json).unwrap_or_default(),
            })
        })?;

        let mut prs = Vec::new();
        for pr in rows {
            prs.push(pr?);
        }
        Ok(prs)
    }
}
