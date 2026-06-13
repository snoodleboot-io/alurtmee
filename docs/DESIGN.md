# Alurtmee — Design & Plan

A pure-Rust desktop app that polls your GitHub account and surfaces, filters, and flags:
test results, PR comments, and open pull requests — with human/bot and feature/security
differentiation, and alerts when CI/CD runs take too long.

**Privacy invariant:** runs entirely on the user's machine. No telemetry, no server, nothing
sensitive ever phoned home. The only outbound traffic is to `api.github.com` on the user's behalf.

**Last updated:** 2026-06-12

---

## 1. Decisions (locked)

| Area | Decision | Rationale |
|------|----------|-----------|
| Language | 100% Rust | Per requirement. |
| GUI | **Iced** | Retained-mode, GPU-rendered, redraws only on state change → low idle CPU. Modern look, mature. |
| Real-time | **Poll** (no webhooks) | Webhooks need a public HTTPS endpoint = a server. Incompatible with "runs on user's machine / nothing phoned home". GitHub optimizes polling instead (see §3). |
| Auth | **Fine-grained PAT** | No app registration; user scopes the token to chosen repos/orgs. Stored in OS keychain. |
| Feat vs Security | **Layered classifier** | Labels → title/branch prefix → changed paths → Dependabot/advisory linkage. First confident signal wins; record which fired. |
| Human vs Bot | Automatic | `user.type == "Bot"` or login ends in `[bot]`; plus user-editable bot allowlist. |
| OS targets | Linux, macOS, Windows | Core arch is identical; only packaging differs. |
| Storage | Local SQLite (bundled) | Cache + ETags + baselines + config. PAT lives in keychain, never in the DB. |

---

## 2. What we track (domain)

Three primary item kinds, each carrying filterable **dimensions**:

- **Open Pull Requests** — author, labels, category, age, draft state, review state.
- **Test Results** — check runs / workflow conclusions / status checks on each PR's head commit
  (pass / fail / pending) plus the standalone "slow CI" flag.
- **PR Comments** — issue comments + review comments, attributed to author.

Every item is tagged with these dimensions so filtering is a single composable predicate:

```
source:    Human | Bot
kind:      PullRequest | TestResult | Comment
category:  Feature | Security | Other        (PRs; with confidence + signal that fired)
scope:     org / repo
flags:     SlowCI, Failing, NeedsReview, ...
```

Filtering is then just `items.filter(|i| active_filters.matches(i))` — chips in the UI toggle
predicate terms (Human/Bot, Feature/Security, Failing, SlowCI, per org/repo).

---

## 3. Real-time strategy (the "explore push vs poll" answer)

**GitHub offers no push channel a desktop client can use.** Webhooks require a publicly
reachable HTTPS endpoint to receive deliveries. So we poll — but cheaply:

1. **Conditional requests.** Every REST response carries `ETag` (and often `Last-Modified`).
   We store them and send `If-None-Match` / `If-Modified-Since`. A `304 Not Modified`
   **does not count against the rate limit** — idle polling is effectively free.
2. **`X-Poll-Interval`.** The `/notifications` and `/events` endpoints return a server-dictated
   minimum poll interval; we obey it and back off under load.
3. **Adaptive cadence.** Poll ~60s when the window is focused, slow to several minutes when
   backgrounded/idle. This is the main low-system-impact lever alongside 304s.
4. **Two-tier polling:**
   - *Cheap change-detection tier* — `GET /notifications` + per-repo `GET /pulls?state=open`,
     all conditional. Most cycles return 304 and cost nothing.
   - *Enrichment tier* — only for PRs that actually changed, fetch reviews/comments/check-runs.
     Starts as REST; can move to a single batched GraphQL query per repo if request volume grows.

**Rate-limit headroom:** authenticated REST = 5,000 req/hr; 304s are free. Polling 20 repos
every 60s is ~1,200 req/hr of which the vast majority are 304. Comfortable margin.

### Endpoints
| Need | Endpoint |
|------|----------|
| Open PRs | `GET /repos/{o}/{r}/pulls?state=open&per_page=100` |
| Reviews | `GET /repos/{o}/{r}/pulls/{n}/reviews` |
| Review comments | `GET /repos/{o}/{r}/pulls/{n}/comments` |
| Issue comments | `GET /repos/{o}/{r}/issues/{n}/comments` |
| Test results | `GET /repos/{o}/{r}/commits/{sha}/check-runs`, `.../status` |
| CI timing | `GET /repos/{o}/{r}/actions/runs` (+ `/jobs` for per-job timing) |
| Change signal | `GET /notifications` (honor `X-Poll-Interval`) |
| Scope discovery | `GET /user/orgs`, `GET /orgs/{org}/repos`, `GET /user/repos` |

---

## 4. Classification

### Human vs Bot
`author.type == "Bot"` OR `login` ends with `[bot]` (e.g. `dependabot[bot]`,
`github-actions[bot]`). User-editable allow/deny list for edge cases (e.g. a service account
that's technically a `User`).

### Feature vs Security (layered — first confident signal wins, all recorded)
1. **Labels** — configurable map, e.g. `{security, vuln, cve} → Security`, `{feature, enhancement} → Feature`.
2. **Title / branch prefix** — `security/*`, `feat/*`; `feat:`, `fix(sec):`, conventional-commit style.
3. **Changed paths** — touches sensitive paths (`**/auth/**`, lockfiles, `**/crypto/**`, CI config) → Security candidate.
4. **Dependabot / advisory linkage** — Dependabot *security* updates or PRs linked to a security advisory → Security.

Output: `Category { kind, confidence, signal }` so the UI can show *why* something was flagged
and let the user correct it (correction feeds the per-repo config).

### "CI/CD taking TOO long"
Per workflow run/job, `duration = completed_at − started_at`. Flag when either:
- **Fixed threshold** the user sets per workflow (e.g. "build > 10 min"), or
- **Rolling baseline** — duration exceeds p75/p90 of the last N runs of that
  `(repo, workflow)` (history kept in SQLite). Default mode; no config required.

Flag carries the reason ("2.4× slower than 30-run median") for the alert text.

---

## 5. Architecture

Cargo **workspace**, separation for testability:

```
alurtmee/
├─ Cargo.toml                 # workspace
├─ docs/DESIGN.md             # this file
└─ crates/
   ├─ gh-client/   # GitHub: auth, REST w/ conditional requests, optional GraphQL,
   │               # rate-limit + X-Poll-Interval handling, typed responses
   ├─ domain/      # core types + classifiers (Author, Category, Item, filters) — pure, no I/O
   ├─ store/       # SQLite: cache, ETag store, CI baselines, config; keychain wrapper
   ├─ poller/      # async scheduler, diff/change-detection engine, emits domain events
   └─ app/         # Iced UI + binary; wires poller→store→UI, native notifications
```

**Data flow:** `poller` (tokio) polls via `gh-client`, persists through `store`, diffs against
cache, emits change events → bridged into Iced via a `subscription` → UI updates (retained, so
only changed widgets redraw) → optional native desktop notification.

### Crate stack
| Concern | Crate |
|---------|-------|
| GUI | `iced` |
| Async runtime | `tokio` |
| HTTP (conditional-request control) | `reqwest` |
| Optional batched fetch | `graphql_client` or `cynic` |
| Local DB (bundled, no system dep) | `rusqlite` (bundled SQLite) |
| Secret storage | `keyring` (OS keychain / Secret Service / Credential Manager) |
| Native notifications | `notify-rust` (+ mac backend) |
| Config dirs | `directories` |
| Serde / config | `serde`, `serde_json`, `toml` |
| Logging | `tracing`, `tracing-subscriber` |
| Errors | `thiserror`, `anyhow` |

### Storage sketch
```
config(key, value)                         -- selected orgs/repos, thresholds, label maps, filters
etags(endpoint PK, etag, last_modified)    -- survives restarts → free 304s on launch
pull_requests(id, repo, number, author, author_type, category, signal, state, draft, updated_at, ...)
checks(pr_id, name, status, conclusion, started_at, completed_at)
comments(id, pr_id, author, author_type, kind, created_at, body_preview)
ci_runs(repo, workflow, run_id, started_at, completed_at, duration_s)   -- baseline history
```
PAT is **never** stored here — it lives in the OS keychain via `keyring`.

---

## 6. UI (Iced)

- **Single dashboard / unified feed** of items, newest-relevant first.
- **Filter bar** = toggle chips: `Human | Bot`, `Feature | Security | Other`,
  `Open PRs | Tests | Comments`, `Failing`, `Slow CI`, plus org/repo selectors. Composable.
- **Bot vs human** visually distinct (icon + muted styling) and filterable in one click.
- **Security** items visually prioritized (accent/badge).
- **Slow CI** items badged with the reason.
- **Settings**: paste PAT, pick orgs/repos, label→category map, CI thresholds, poll cadence,
  notification toggles.
- Retained-mode + change-driven redraw + adaptive poll cadence = the "modern feel, low impact" goal.
- Optional tray icon + native notifications for: new test failure, slow CI, new review/comment.

---

## 7. Phased build plan

**Phase 0 — Skeleton.** Workspace, crates, CI for the three OSes, `tracing`, empty Iced window.

**Phase 1 — Auth + scope.** PAT entry → keychain; validate token; list orgs/repos; persist
selection. *Milestone: app authenticates and shows your repos.*

**Phase 2 — Poller core.** `gh-client` REST + conditional requests + rate-limit handling;
`store` cache + ETag persistence; poll open PRs for selected repos; show a raw list.
*Milestone: open PRs appear and refresh cheaply (304s observed in logs).*

**Phase 3 — Enrichment.** Reviews, comments, check runs/test results per PR. Render test
pass/fail/pending and comments. *Milestone: full PR detail in the feed.*

**Phase 4 — Classification.** Human/bot tagging; layered feature/security classifier with
user correction. *Milestone: items correctly tagged and filterable.*

**Phase 5 — CI timing + alerts.** Pull `actions/runs`, build rolling baselines, slow-CI flag,
native notifications. *Milestone: slow runs flagged and notified.*

**Phase 6 — Filters + polish.** Composable filter chips, adaptive cadence, settings UI,
empty/error states, theming. *Milestone: the experience described above.*

**Phase 7 — Packaging.** Per-OS bundles (Linux AppImage/deb, macOS .app + notarization,
Windows MSI). *Milestone: installable artifacts.*

---

## 8. Open questions / deferred

- GraphQL enrichment: defer until REST request volume proves it's needed.
- GitHub Enterprise (custom base URL) support: easy to add later; out of v1 scope unless needed.
- Multi-account: single account v1; the keychain/config model leaves room to extend.
- Sync of "read/seen" state across devices: out of scope (privacy invariant; local-only).
