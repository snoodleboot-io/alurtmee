//! CI timing persistence: recorded workflow-run outcomes and recent-duration lookups that feed the
//! slow-CI baseline.

use rusqlite::params;

use crate::error::StoreError;
use crate::store::Store;

impl Store {
    /// Record a single workflow run, keyed by `(repo, run_id)`.
    ///
    /// Returns `true` when a new row was inserted and `false` when the run was already recorded
    /// (the `(repo, run_id)` pair already existed). The insert is idempotent via
    /// `ON CONFLICT … DO NOTHING`, so re-recording a known run is a no-op. `conclusion` is stored as
    /// nullable TEXT (`None` for an in-progress run). Only non-secret run metadata is written — never
    /// a token (ARD AD-6).
    pub fn record_run(&self, run: &domain::WorkflowRun) -> Result<bool, StoreError> {
        self.conn.execute(
            "INSERT INTO ci_runs (repo, run_id, workflow, conclusion, duration_secs)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(repo, run_id) DO NOTHING",
            params![
                run.repo,
                run.id as i64,
                run.workflow,
                run.conclusion,
                run.duration_secs as i64,
            ],
        )?;
        Ok(self.conn.changes() == 1)
    }

    /// Return the durations of the most recent completed runs for `(repo, workflow)`.
    ///
    /// Only runs with a non-null `conclusion` (i.e. finished runs) are considered; in-progress runs
    /// are excluded. Results are ordered newest-first by `run_id` and capped at `limit`. A
    /// never-recorded `(repo, workflow)` yields an empty `Vec`.
    pub fn recent_durations(
        &self,
        repo: &str,
        workflow: &str,
        limit: usize,
    ) -> Result<Vec<u64>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT duration_secs FROM ci_runs
             WHERE repo = ?1 AND workflow = ?2 AND conclusion IS NOT NULL
             ORDER BY run_id DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![repo, workflow, limit as i64], |row| {
            let secs: i64 = row.get(0)?;
            Ok(secs as u64)
        })?;

        let mut durations = Vec::new();
        for duration in rows {
            durations.push(duration?);
        }
        Ok(durations)
    }
}
