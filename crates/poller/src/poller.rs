use std::time::Duration;

use domain::{
    classify_category, AuthorKind, ChangeEvent, Classification, ClassificationInput, LabelMap,
    PollCadence, PrEnrichment, PullRequest,
};
use gh_client::{GhClient, PrOutcome};
use store::{EtagRecord, Store};
use tokio::sync::mpsc::Sender;

use crate::diff::diff_pull_requests;
use crate::poll_error::PollError;
use crate::poll_outcome::PollOutcome;

/// The two-tier poller (ARD AD-3).
///
/// **Why two tiers, not refetch-all (§3.6):** the cheap tier is a single conditional request per
/// repo that 304s for free when nothing changed; the expensive tier (reviews + comments +
/// check-runs, several requests per PR) fires *only* for PRs the diff flagged as changed. Refetching
/// every PR's enrichment each cycle would burn rate limit and CPU on PRs that didn't move — exactly
/// what AD-3 avoids. So enrichment is gated behind change-detection here.
///
/// Owns its own [`GhClient`] and [`Store`] connection so it can run on a background task without
/// sharing mutable state with the UI; it communicates results purely as [`ChangeEvent`]s over a
/// channel. One [`Self::poll_once`] is the unit of behaviour (conditionally fetch each selected
/// repo, diff against the cache, enrich the changed PRs, persist), which is what the acceptance
/// tests drive; [`Self::run`] is the thin scheduling shell around it.
pub struct Poller {
    client: GhClient,
    store: Store,
    cadence: PollCadence,
}

impl Poller {
    /// Construct a poller from an authenticated client, an open store, and a cadence policy.
    pub fn new(client: GhClient, store: Store, cadence: PollCadence) -> Self {
        Self {
            client,
            store,
            cadence,
        }
    }

    /// The etag cache key for a repository's open-PR listing.
    fn endpoint_key(repo: &str) -> String {
        format!("pulls:{repo}")
    }

    /// Run a single poll cycle across `repos`: for each, send a conditional request keyed on the
    /// persisted ETag, and on a `200` diff the fresh list against the cache (emitting events and
    /// updating the cache). A `304` leaves cached state untouched and emits nothing.
    pub async fn poll_once(&mut self, repos: &[String]) -> Result<PollOutcome, PollError> {
        let mut outcome = PollOutcome::default();

        for repo in repos {
            let endpoint = Self::endpoint_key(repo);
            let prior_etag = self.store.get_etag(&endpoint)?.and_then(|r| r.etag);

            let result = self
                .client
                .list_open_prs(repo, prior_etag.as_deref())
                .await?;

            // Track the server's cadence floor (largest hint) and the latest budget snapshot.
            if let Some(hint) = result.poll_interval {
                outcome.poll_interval = Some(match outcome.poll_interval {
                    Some(existing) => existing.max(hint),
                    None => hint,
                });
            }
            if result.rate_limit.is_some() {
                outcome.rate_limit = result.rate_limit;
            }

            // Persist the refreshed ETag (the client carries the prior one forward on a 304).
            if result.etag.is_some() {
                self.store.set_etag(
                    &endpoint,
                    &EtagRecord {
                        etag: result.etag.clone(),
                        last_modified: None,
                    },
                )?;
            }

            if let PrOutcome::Modified(fresh) = result.outcome {
                let cached = self.store.load_repo_prs(repo)?;
                let diff = diff_pull_requests(&cached, &fresh);
                self.store.cache_repo_prs(repo, &fresh)?;
                if !diff.is_empty() {
                    outcome.changed = true;
                }

                // Enrichment tier (AD-3): fetch reviews/comments/check-runs ONLY for the PRs the
                // diff flagged as added/updated. A 304 never reaches here, and unchanged PRs are
                // skipped, so enrichment work is proportional to what actually changed.
                let mut derived = Vec::new();
                for event in &diff {
                    let pr = match event {
                        ChangeEvent::Added(pr) | ChangeEvent::Updated(pr) => pr,
                        _ => continue,
                    };
                    let enrichment = self.enrich_pr(pr).await?;
                    self.store.save_enrichment(&enrichment)?;
                    derived.push(ChangeEvent::Enriched(enrichment));

                    let classification = self.classify_pr(pr).await?;
                    derived.push(ChangeEvent::Classified(classification));
                }

                outcome.events.extend(diff);
                outcome.events.extend(derived);
            }
        }

        Ok(outcome)
    }

    /// Fetch and assemble the enrichment (reviews, merged comments, reconciled CI verdict) for one
    /// changed PR. The check-runs/status lookup is keyed on the PR's head SHA.
    ///
    /// Takes `&mut self` (not `&self`) deliberately: the returned future is spawned on a background
    /// task and must be `Send`, but a shared `&Poller` is `!Send` because the store's SQLite
    /// `Connection` is `!Sync`. A unique `&mut Poller` only requires `Poller: Send`, which holds.
    async fn enrich_pr(&mut self, pr: &PullRequest) -> Result<PrEnrichment, PollError> {
        let reviews = self.client.list_reviews(&pr.id.repo, pr.id.number).await?;
        let comments = self.client.list_comments(&pr.id.repo, pr.id.number).await?;
        let tests = self.client.test_summary(&pr.id.repo, &pr.head_sha).await?;
        Ok(PrEnrichment::new(pr.id.clone(), reviews, comments, tests))
    }

    /// Classify one changed PR (AD-5): fetch its changed paths, load the per-repo label map, bot
    /// overrides, and any user correction from the store, then run the pure classifiers. `&mut self`
    /// for the same `Send` reason as [`Self::enrich_pr`].
    async fn classify_pr(&mut self, pr: &PullRequest) -> Result<Classification, PollError> {
        let repo = &pr.id.repo;
        let changed_paths = self.client.list_changed_paths(repo, pr.id.number).await?;
        let label_map = self
            .store
            .load_label_map(repo)?
            .unwrap_or_else(LabelMap::with_common_defaults);
        let bot_overrides = self.store.load_bot_overrides(repo)?.unwrap_or_default();
        let correction = self.store.get_correction(repo, pr.id.number)?;

        let author_kind = AuthorKind::classify(&pr.author, &pr.author_type, &bot_overrides);
        let input = ClassificationInput {
            author_login: &pr.author,
            title: &pr.title,
            head_ref: &pr.head_ref,
            labels: &pr.labels,
            changed_paths: &changed_paths,
        };
        let category = classify_category(&input, &label_map, correction);

        Ok(Classification {
            id: pr.id.clone(),
            author_kind,
            category,
        })
    }

    /// Drive the poll loop, streaming each [`ChangeEvent`] over `tx` until the consumer drops the
    /// receiver. The interval adapts: it resets to the cadence base whenever a change is seen and
    /// backs off exponentially otherwise, never below the server's `X-Poll-Interval` hint, with
    /// jitter applied to avoid synchronized bursts. Cancellation-safe: dropping this future (e.g.
    /// an Iced subscription being torn down) aborts cleanly at the next await point, and a closed
    /// channel ends the loop without further work.
    pub async fn run(mut self, repos: Vec<String>, tx: Sender<ChangeEvent>) {
        let mut consecutive_unchanged: u32 = 0;

        loop {
            if tx.is_closed() {
                return;
            }

            let outcome = match self.poll_once(&repos).await {
                Ok(outcome) => outcome,
                Err(err) => {
                    tracing::warn!("poll cycle failed, backing off: {err}");
                    PollOutcome::default()
                }
            };

            if let Some(rate_limit) = outcome.rate_limit {
                tracing::debug!(
                    remaining = rate_limit.remaining,
                    limit = rate_limit.limit,
                    "github rate limit"
                );
            }

            for event in &outcome.events {
                if tx.send(event.clone()).await.is_err() {
                    return; // consumer dropped
                }
            }

            consecutive_unchanged = if outcome.changed {
                0
            } else {
                consecutive_unchanged.saturating_add(1)
            };

            let interval = apply_jitter(
                self.cadence
                    .interval(consecutive_unchanged, outcome.poll_interval),
                jitter_fraction(),
            );
            tokio::time::sleep(interval).await;
        }
    }
}

/// A pseudo-random fraction in `[0, 1)` derived from the current time's sub-second component.
///
/// Jitter only needs to de-correlate timers across processes, not cryptographic randomness, so a
/// cheap time-based source avoids pulling in an RNG dependency.
fn jitter_fraction() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1_000) as f64 / 1_000.0
}

/// Add up to +25% of `base` to the interval, scaled by `fraction` (clamped to `[0, 1]`).
///
/// Jitter only ever *lengthens* the wait, so it can never make us poll faster than intended, and
/// it can never overflow.
fn apply_jitter(base: Duration, fraction: f64) -> Duration {
    let scaled = fraction.clamp(0.0, 1.0) * 0.25;
    base + Duration::from_secs_f64(base.as_secs_f64() * scaled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{PrId, PullRequest};
    use wiremock::matchers::{header, method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cadence() -> PollCadence {
        PollCadence::new(Duration::from_millis(20), Duration::from_millis(100))
    }

    /// Mount empty/default responses for every enrichment + classification endpoint so a changed
    /// PR enriches and classifies cleanly. Path-regex matchers cover any PR number / head SHA.
    async fn mount_enrichment_defaults(server: &MockServer) {
        for suffix in [
            r"/reviews$",
            r"/pulls/\d+/comments$",
            r"/issues/\d+/comments$",
            r"/files$",
        ] {
            Mock::given(method("GET"))
                .and(path_regex(suffix))
                .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
                .mount(server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path_regex(r"/check-runs$"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"check_runs":[]}"#))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"/status$"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"state":"success"}"#))
            .mount(server)
            .await;
    }

    /// Count events of each kind for concise assertions: (added, updated, removed, enriched,
    /// classified).
    fn count_kinds(events: &[ChangeEvent]) -> (usize, usize, usize, usize, usize) {
        let mut added = 0;
        let mut updated = 0;
        let mut removed = 0;
        let mut enriched = 0;
        let mut classified = 0;
        for event in events {
            match event {
                ChangeEvent::Added(_) => added += 1,
                ChangeEvent::Updated(_) => updated += 1,
                ChangeEvent::Removed(_) => removed += 1,
                ChangeEvent::Enriched(_) => enriched += 1,
                ChangeEvent::Classified(_) => classified += 1,
            }
        }
        (added, updated, removed, enriched, classified)
    }

    fn pulls_body() -> &'static str {
        r#"[
            {"number":1,"title":"first","user":{"login":"octocat"},"draft":false,"updated_at":"t1","html_url":"https://github.com/o/r/pull/1"},
            {"number":2,"title":"second","user":{"login":"hubot"},"draft":true,"updated_at":"t1","html_url":"https://github.com/o/r/pull/2"}
        ]"#
    }

    fn pulls_body_changed() -> &'static str {
        r#"[
            {"number":1,"title":"first","user":{"login":"octocat"},"draft":false,"updated_at":"t2","html_url":"https://github.com/o/r/pull/1"}
        ]"#
    }

    fn poller_for(server: &MockServer) -> Poller {
        let client = GhClient::new(server.uri(), "dummy-token").unwrap();
        let store = Store::open_in_memory().unwrap();
        Poller::new(client, store, cadence())
    }

    #[tokio::test]
    async fn first_poll_emits_added_for_each_pr_and_caches_them() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"v1\"")
                    .set_body_string(pulls_body()),
            )
            .mount(&server)
            .await;
        mount_enrichment_defaults(&server).await;

        let mut poller = poller_for(&server);
        let outcome = poller.poll_once(&["o/r".to_string()]).await.unwrap();

        assert!(outcome.changed);
        // Two PRs appear (Added) and each is enriched (Enriched).
        let (added, _, _, enriched, classified) = count_kinds(&outcome.events);
        assert_eq!(added, 2);
        assert_eq!(enriched, 2);
        assert_eq!(classified, 2, "each changed PR is also classified");
        assert!(matches!(&outcome.events[0], ChangeEvent::Added(p) if p.id.number == 1));
        // The fetched set is now cached.
        assert_eq!(poller.store.load_repo_prs("o/r").unwrap().len(), 2);
    }

    #[tokio::test]
    async fn unchanged_repoll_returns_304_with_no_events() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls"))
            .and(header("if-none-match", "\"v1\""))
            .respond_with(ResponseTemplate::new(304).insert_header("ETag", "\"v1\""))
            .mount(&server)
            .await;

        let mut poller = poller_for(&server);
        // Seed the persisted ETag and a cached PR so the conditional request fires.
        poller
            .store
            .set_etag(
                &Poller::endpoint_key("o/r"),
                &EtagRecord {
                    etag: Some("\"v1\"".to_string()),
                    last_modified: None,
                },
            )
            .unwrap();
        poller
            .store
            .cache_repo_prs(
                "o/r",
                &[PullRequest {
                    id: PrId::new("o/r", 1),
                    title: "first".to_string(),
                    author: "octocat".to_string(),
                    draft: false,
                    updated_at: "t1".to_string(),
                    url: "https://github.com/o/r/pull/1".to_string(),
                    head_sha: String::new(),
                    author_type: String::new(),
                    head_ref: String::new(),
                    labels: Vec::new(),
                }],
            )
            .unwrap();

        let outcome = poller.poll_once(&["o/r".to_string()]).await.unwrap();

        assert!(!outcome.changed, "304 must not be reported as a change");
        assert!(outcome.events.is_empty());
        // Cached state is undisturbed by the 304.
        assert_eq!(poller.store.load_repo_prs("o/r").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn changed_body_produces_update_and_remove_events() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"v2\"")
                    .set_body_string(pulls_body_changed()),
            )
            .mount(&server)
            .await;
        mount_enrichment_defaults(&server).await;

        let mut poller = poller_for(&server);
        // Cache the prior two PRs; the fresh body advances #1 and drops #2.
        poller
            .store
            .cache_repo_prs(
                "o/r",
                &[
                    PullRequest {
                        id: PrId::new("o/r", 1),
                        title: "first".to_string(),
                        author: "octocat".to_string(),
                        draft: false,
                        updated_at: "t1".to_string(),
                        url: "https://github.com/o/r/pull/1".to_string(),
                        head_sha: String::new(),
                        author_type: String::new(),
                        head_ref: String::new(),
                        labels: Vec::new(),
                    },
                    PullRequest {
                        id: PrId::new("o/r", 2),
                        title: "second".to_string(),
                        author: "hubot".to_string(),
                        draft: true,
                        updated_at: "t1".to_string(),
                        url: "https://github.com/o/r/pull/2".to_string(),
                        head_sha: String::new(),
                        author_type: String::new(),
                        head_ref: String::new(),
                        labels: Vec::new(),
                    },
                ],
            )
            .unwrap();

        let outcome = poller.poll_once(&["o/r".to_string()]).await.unwrap();

        assert!(outcome.changed);
        assert!(outcome.events.contains(&ChangeEvent::Updated(PullRequest {
            id: PrId::new("o/r", 1),
            title: "first".to_string(),
            author: "octocat".to_string(),
            draft: false,
            updated_at: "t2".to_string(),
            url: "https://github.com/o/r/pull/1".to_string(),
            head_sha: String::new(),
            author_type: String::new(),
            head_ref: String::new(),
            labels: Vec::new(),
        })));
        assert!(outcome
            .events
            .contains(&ChangeEvent::Removed(PrId::new("o/r", 2))));
        // Only the surviving/changed PR (#1) is enriched; the removed #2 is not.
        let (_, _, _, enriched, classified) = count_kinds(&outcome.events);
        assert_eq!(enriched, 1);
        assert_eq!(classified, 1, "only the changed PR is classified");
    }

    #[tokio::test]
    async fn run_streams_events_then_stops_when_receiver_drops() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"v1\"")
                    .set_body_string(pulls_body()),
            )
            .mount(&server)
            .await;
        mount_enrichment_defaults(&server).await;

        let poller = poller_for(&server);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let handle = tokio::spawn(poller.run(vec!["o/r".to_string()], tx));

        // First cycle emits the two Added events.
        let first = rx.recv().await.unwrap();
        let second = rx.recv().await.unwrap();
        assert!(matches!(first, ChangeEvent::Added(_)));
        assert!(matches!(second, ChangeEvent::Added(_)));

        // Dropping the receiver must end the loop promptly (cancellation-safety).
        drop(rx);
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("run loop should terminate after receiver drop")
            .expect("task should not panic");
    }

    #[test]
    fn apply_jitter_only_lengthens_within_25_percent() {
        let base = Duration::from_secs(100);
        assert_eq!(apply_jitter(base, 0.0), base, "no jitter at fraction 0");
        assert_eq!(
            apply_jitter(base, 1.0),
            Duration::from_secs(125),
            "max +25% at fraction 1"
        );
        let mid = apply_jitter(base, 0.5);
        assert!(mid >= base && mid <= Duration::from_secs(125));
    }
}
