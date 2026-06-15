//! Demo seed (manual UI review only) — gated behind the `ALURTMEE_DEMO` env var in `main`.
//!
//! Produces a handful of sample [`ChangeEvent`]s (PRs + enrichment) so the dashboard can be
//! eyeballed without a GitHub token or a live poll. This is a development/review aid, not part of
//! the product flow: nothing here runs unless `ALURTMEE_DEMO` is set.

use domain::{
    ChangeEvent, Comment, CommentKind, PrEnrichment, PrId, PullRequest, Review, TestState,
    TestSummary,
};

/// The sequence of events a real poll cycle would emit for two illustrative PRs — one green, one
/// failing — each followed by its enrichment.
pub fn demo_events() -> Vec<ChangeEvent> {
    let pr_green = PullRequest {
        id: PrId::new("octocat/hello", 42),
        title: "Add dashboard filter".to_string(),
        author: "octocat".to_string(),
        draft: false,
        updated_at: "2026-06-14T09:00:00Z".to_string(),
        url: "https://github.com/octocat/hello/pull/42".to_string(),
        head_sha: "aaa111".to_string(),
    };
    let green_enrichment = PrEnrichment::new(
        pr_green.id.clone(),
        vec![
            Review {
                author: "alice".to_string(),
                state: "APPROVED".to_string(),
                submitted_at: "2026-06-14T09:10:00Z".to_string(),
            },
            Review {
                author: "bob".to_string(),
                state: "COMMENTED".to_string(),
                submitted_at: "2026-06-14T09:12:00Z".to_string(),
            },
        ],
        vec![Comment {
            author: "carol".to_string(),
            kind: CommentKind::Issue,
            body: "Looks good — ship it once CI is green.".to_string(),
            created_at: "2026-06-14T09:15:00Z".to_string(),
        }],
        TestSummary {
            passed: 3,
            failed: 0,
            pending: 0,
            state: TestState::Passing,
        },
    );

    let pr_red = PullRequest {
        id: PrId::new("octocat/hello", 43),
        title: "Fix flaky CI on Windows".to_string(),
        author: "hubot".to_string(),
        draft: true,
        updated_at: "2026-06-14T10:30:00Z".to_string(),
        url: "https://github.com/octocat/hello/pull/43".to_string(),
        head_sha: "bbb222".to_string(),
    };
    let red_enrichment = PrEnrichment::new(
        pr_red.id.clone(),
        vec![Review {
            author: "bob".to_string(),
            state: "CHANGES_REQUESTED".to_string(),
            submitted_at: "2026-06-14T10:40:00Z".to_string(),
        }],
        vec![Comment {
            author: "dave".to_string(),
            kind: CommentKind::Review,
            body: "This retry loop can spin forever; add a timeout.".to_string(),
            created_at: "2026-06-14T10:45:00Z".to_string(),
        }],
        TestSummary {
            passed: 1,
            failed: 1,
            pending: 0,
            state: TestState::Failing,
        },
    );

    vec![
        ChangeEvent::Added(pr_green),
        ChangeEvent::Enriched(green_enrichment),
        ChangeEvent::Added(pr_red),
        ChangeEvent::Enriched(red_enrichment),
    ]
}
