# ARD — ALRT-1: Alurtmee Architecture

**Status:** Draft (awaiting approval)
**Ticket:** ALRT-1
**Created:** 2026-06-13
**Related:** [ALRT-1-PRD.md](./ALRT-1-PRD.md)

Architecture Decision Record. Each decision lists the choice, alternatives considered, and the
tradeoff. Binds to the project Rust conventions (`thiserror`/`anyhow`, `Result` everywhere,
traits for abstraction, derive `Debug`/`Clone`/`Serialize`/`Deserialize`, snake_case files,
one-type-per-file, SOLID).

---

## AD-1 — Push vs Poll: **Poll with conditional requests**

**Decision:** Poll the GitHub REST API; do not use webhooks.

**Why:** Webhooks require a publicly reachable HTTPS endpoint to receive deliveries — i.e. a
server — which violates the privacy invariant (PRD §2). GitHub has no push channel a local
desktop client can subscribe to. GitHub instead optimizes polling:
- **Conditional requests** (`ETag`/`If-None-Match`, `Last-Modified`/`If-Modified-Since`):
  a `304 Not Modified` **does not count against the rate limit** — idle polling is effectively free.
- **`X-Poll-Interval`** on `/notifications` and `/events` dictates a minimum interval we obey.
- **Adaptive cadence** (~60s focused, minutes when backgrounded) bounds cost and CPU.

**Alternatives:**
- *Webhooks* — rejected (needs a public server; breaks privacy invariant).
- *GraphQL-only polling* — costs rate-limit points on every call (no free 304s); deferred to
  enrichment only (AD-3).

**Rate-limit headroom:** authed REST = 5,000 req/hr; 304s free. ~20 repos polled every 60s ≈
1,200 req/hr, mostly 304. Comfortable.

## AD-2 — GUI: **Iced**

**Decision:** Build the UI in Iced.

**Why:** Pure Rust, retained-mode, GPU-rendered; redraws only on state change → low idle CPU
(directly serves NFR2). Mature, modern look, good fit for a dashboard/feed.

**Alternatives:** *Slint* (smallest footprint, but its own DSL); *egui* (simplest, but
immediate-mode redraws every frame → more idle CPU); *Dioxus* (React-like, but desktop runs in
a system WebView — heavier, less "pure native render"). Tradeoff: Iced balances modern feel,
pure-Rust native rendering, and low idle cost.

## AD-3 — Two-tier polling (cheap change-detection + targeted enrichment)

**Decision:**
- *Change-detection tier* — `GET /notifications` + per-repo `GET /pulls?state=open`, all
  conditional. Most cycles 304 → free.
- *Enrichment tier* — only for PRs that changed, fetch reviews/comments/check-runs. Start REST;
  move to a single batched **GraphQL** query per repo only if request volume proves it
  necessary (PRD OQ1).

**Why:** Minimizes both rate-limit cost and CPU; keeps the common (nothing-changed) path nearly
free. Tradeoff: more moving parts than naive "fetch everything each cycle," but far cheaper.

### Endpoints
| Need | Endpoint |
|------|----------|
| Open PRs | `GET /repos/{o}/{r}/pulls?state=open&per_page=100` |
| Reviews | `GET /repos/{o}/{r}/pulls/{n}/reviews` |
| Review comments | `GET /repos/{o}/{r}/pulls/{n}/comments` |
| Issue comments | `GET /repos/{o}/{r}/issues/{n}/comments` |
| Test results | `GET /repos/{o}/{r}/commits/{sha}/check-runs`, `.../status` |
| CI timing | `GET /repos/{o}/{r}/actions/runs` (+ `/jobs`) |
| Change signal | `GET /notifications` (honor `X-Poll-Interval`) |
| Scope discovery | `GET /user/orgs`, `GET /orgs/{org}/repos`, `GET /user/repos` |

## AD-4 — Auth: **Fine-grained PAT in OS keychain**

**Decision:** User pastes a fine-grained PAT; store via `keyring` (OS keychain / Secret Service /
Credential Manager). Never in the SQLite DB or plaintext.

**Alternatives:** *OAuth device flow* (nicer UX but needs a registered public app/client ID);
*GitHub App* (granular + higher limits but heaviest setup). Tradeoff: PAT is simplest, requires
no registration, and keeps everything local.

## AD-5 — Classification engines (pure, in `domain`)

- **Human vs Bot:** `author.type == "Bot"` OR login ends `[bot]`; user allow/deny overrides.
- **Feature vs Security (layered, first confident wins, all recorded):**
  1. Labels (configurable map), 2. Title/branch prefix (`security/*`, `feat:`, `fix(sec):`),
  3. Changed paths (auth/, lockfiles, crypto/, CI config), 4. Dependabot/advisory linkage.
  Output `Category { kind, confidence, signal }`; user corrections feed per-repo config.
- **Slow CI:** `duration = completed_at − started_at`; flag if > fixed threshold OR > p75/p90 of
  last N runs for `(repo, workflow)`. Flag carries the reason string.

Tradeoff: heuristic, not perfect — mitigated by recording the firing signal and allowing
correction. Engines are pure (no I/O) → fully unit-testable.

## AD-6 — Storage: **bundled SQLite + keychain**

**Decision:** `rusqlite` (bundled SQLite, zero system dependency) for cache, ETags, CI baselines,
config. PAT in keychain only.

**Why:** Persisted ETags survive restarts → immediate cheap re-sync (free 304s on launch).
Cached items render instantly and let us diff to detect "new". Bundled = no external DB to
install. Alternative `sqlx` (async) deferred — `rusqlite` on a dedicated DB task is simpler.

### Schema sketch
```
config(key, value)
etags(endpoint PK, etag, last_modified)
pull_requests(id, repo, number, author, author_type, category, signal, state, draft, updated_at, ...)
checks(pr_id, name, status, conclusion, started_at, completed_at)
comments(id, pr_id, author, author_type, kind, created_at, body_preview)
ci_runs(repo, workflow, run_id, started_at, completed_at, duration_s)
```

## AD-7 — Crate layout (Cargo workspace)

```
alurtmee/
└─ crates/
   ├─ gh-client/   # auth, REST + conditional requests, optional GraphQL, rate-limit/X-Poll-Interval
   ├─ domain/      # pure types + classifiers (Author, Category, Item, filters); no I/O
   ├─ store/       # SQLite cache/ETags/baselines/config; keychain wrapper
   ├─ poller/      # async scheduler + diff/change-detection; emits domain events
   └─ app/         # Iced UI + binary; wires poller→store→UI; native notifications
```
**Data flow:** `poller` (tokio) → `gh-client` fetch → `store` persist + diff → emit events →
Iced `subscription` → UI updates (retained; only changed widgets redraw) → optional notification.

Tradeoff: workspace adds structure overhead but gives clean seams and isolates the pure,
testable `domain` from I/O.

## AD-8 — Dependency stack

| Concern | Crate | Note (flag for approval) |
|---------|-------|--------------------------|
| GUI | `iced` | core |
| Async | `tokio` | core |
| HTTP | `reqwest` | needs conditional-request control |
| GraphQL (deferred) | `graphql_client` or `cynic` | only if AD-3 enrichment needs it |
| DB | `rusqlite` (bundled) | zero system dep |
| Secrets | `keyring` | OS keychain |
| Notifications | `notify-rust` (Linux/XDG backend for v1) | optional feature; mac/win backends post-v1 |
| Config dirs | `directories` | |
| Serde/config | `serde`, `serde_json`, `toml` | |
| Logging | `tracing`, `tracing-subscriber` | |
| Errors | `thiserror`, `anyhow` | per Rust conventions |

> Per conventions, **new dependencies must be flagged for approval** — this table is that flag.

## AD-9 — Deferred / revisit

- **macOS/Windows support (R2b)** — v1 is **Linux only**. Deferred to a post-v1 **Phase 8**
  (packaging + notification backends + CI runners + signing). See AD-10 for the portability
  discipline that keeps this additive.
- GraphQL enrichment (AD-3) — adopt when REST volume warrants.
- GitHub Enterprise base URL, multi-account, tray icon (PRD OQ2), write actions — post-v1.
- `sqlx` vs `rusqlite` — revisit if async pressure appears.

## AD-10 — Linux-first with portability seams (R2b)

**Decision:** Build for Linux now, but isolate every platform-specific surface behind a trait or
`#[cfg]` seam so mac/win is additive, not a rewrite.

**Seams:** secrets (`keyring` is already cross-platform), **notifications** (a `Notifier` trait;
Linux/XDG impl via `notify-rust` now), **packaging** (per-OS lanes), **windowing** (Iced/wgpu is
cross-platform; avoid Linux-only window APIs). No Linux-only assumptions leak into `domain`,
`gh-client`, `store`, or `poller` (those are already platform-agnostic).

**Why:** the only genuinely OS-specific work is notifications delivery and packaging; everything
else is portable by construction. A thin trait at that boundary defers the cost without polluting
the core. Tradeoff: one extra abstraction at the notifier boundary now, paid back at Phase 8.

## AD-11 — Mock-first dev, deferred live verification (R2a)

**Decision:** Develop and test against `wiremock` + **recorded real fixtures** with **no PAT**;
defer live `api.github.com` integration verification to a standalone PAT-gated pass
(exec/MASTER §10).

**Why:** lets work start with zero secrets and keeps CI hermetic/fast. Fixtures are *recorded
real payloads* (not invented stubs), so parsers/classifiers are exercised against true shapes.

**Tradeoff (explicit):** until the deferred pass runs, the product is **logic-complete but not
live-verified** — fixtures can drift from API reality. Mitigated by the §10 pass re-running the
same tests live and refreshing fixtures, with divergences treated as findings.
