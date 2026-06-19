# PRD — ALRT-1: Alurtmee (GitHub Poller Desktop App)

**Status:** Draft (awaiting approval)
**Ticket:** ALRT-1
**Author:** plan mode
**Created:** 2026-06-13
**Related:** [ALRT-1-ARD.md](./ALRT-1-ARD.md)

---

## 1. Problem & Motivation

Developers and maintainers need an at-a-glance, low-noise view of what's happening on their
GitHub work — open PRs, the comments on them, and whether CI/tests are passing — without
keeping browser tabs open or being buried in email. They also need to quickly separate
*automation noise* (bots) from *human activity*, and to spot *security* work amid *feature*
work. Existing tools either require a hosted server (webhooks), send data to third parties, or
are heavyweight.

**Alurtmee** is a lightweight, pure-Rust desktop app that polls GitHub on the user's behalf and
surfaces this information locally, with strong filtering and zero data exfiltration.

## 2. Privacy Invariant (non-negotiable)

The app runs entirely on the user's machine. **No telemetry. No backend. Nothing sensitive is
ever phoned home.** The only outbound network traffic is to `api.github.com`, authenticated as
the user. This constraint shapes the architecture (see ARD §"Push vs Poll").

## 3. Target User

A single developer/maintainer running the app on their own desktop (Linux, macOS, or Windows),
who has a GitHub account and works across one or more orgs/repos.

## 4. Goals

- Surface, in one place: **open PRs**, **PR comments**, and **test results**.
- **Flag CI/CD runs that take too long.**
- **Differentiate human vs bot** activity — clearly and filterably.
- **Differentiate feature vs security** PRs.
- **Limit scope** to chosen organizations and repositories.
- **Low system impact**, with a **modern feel**.
- **Local-only**, honoring the privacy invariant.
- **Linux v1**, with code kept portable so macOS/Windows are an additive follow-on.

## 5. Non-Goals (v1)

- No webhook/server mode.
- **No macOS/Windows in v1** — Linux only; mac/win deferred to a post-v1 phase (platform-specific
  surfaces stay behind seams so the port is additive).
- No GitHub Enterprise (custom base URL) — deferred.
- No multi-account — single account in v1 (architecture leaves room).
- No cross-device sync of read/seen state (violates privacy invariant; local-only).
- No write actions to GitHub (merging, commenting) — read/monitor only in v1.

## 6. Functional Requirements

### FR1 — Authentication & Scope
- FR1.1 User authenticates with a **fine-grained Personal Access Token**.
- FR1.2 Token is stored in the **OS keychain**, never on disk in plaintext or in the app DB.
- FR1.3 User selects which **orgs and repos** to monitor; selection persists.

### FR2 — Open Pull Requests
- FR2.1 List open PRs across selected repos with author, labels, age, draft/review state.
- FR2.2 Refresh near-real-time at low cost (see NFR2).

### FR3 — PR Comments
- FR3.1 Show issue comments and review comments per PR, attributed to author.

### FR4 — Test Results
- FR4.1 Show check-run / workflow / status-check outcomes (pass/fail/pending) for each PR's head commit.

### FR5 — Slow CI/CD Flagging
- FR5.1 Flag runs exceeding a threshold. Default = **rolling baseline** (e.g. > p75/p90 of recent
  runs for that workflow); optional **fixed per-workflow threshold**.
- FR5.2 Flag carries a human-readable reason (e.g. "2.4× slower than 30-run median").

### FR6 — Human vs Bot
- FR6.1 Auto-classify each actor as Human or Bot (`type == "Bot"` or login ends `[bot]`).
- FR6.2 User-editable allow/deny list for edge cases.
- FR6.3 Bot vs human is **visually distinct and filterable**.

### FR7 — Feature vs Security
- FR7.1 Classify each PR via a **layered classifier**: labels → title/branch prefix →
  changed paths → Dependabot/advisory linkage; first confident signal wins, all recorded.
- FR7.2 Show *which* signal fired; allow the user to correct (correction feeds per-repo config).
- FR7.3 Security items are **visually prioritized and filterable**.

### FR8 — Filtering
- FR8.1 Composable filters over dimensions: source (Human/Bot), kind (PR/Test/Comment),
  category (Feature/Security/Other), org/repo, flags (Failing, SlowCI, NeedsReview).

### FR9 — Notifications (optional, user-toggle)
- FR9.1 Native desktop notifications for: new test failure, slow CI, new review/comment.

## 7. Non-Functional Requirements

- NFR1 **Pure Rust**, no bundled web runtime in core render path.
- NFR2 **Low system impact**: idle CPU near-zero; polling uses conditional requests so empty
  polls cost no rate limit; redraw only on state change; adaptive poll cadence (faster focused,
  slower backgrounded).
- NFR3 **Modern feel**: clean, responsive UI; clear visual hierarchy.
- NFR4 **Platform**: Linux for v1; code stays cross-platform-clean (platform specifics behind
  traits/`cfg`) so macOS/Windows are an additive post-v1 phase.
- NFR5 **Resilience**: respect `X-Poll-Interval`, exponential backoff + jitter on errors/limits;
  survive restarts (persisted ETags → immediate cheap re-sync).
- NFR6 **Privacy** per §2.

## 8. Success Criteria

- Open PRs, comments, and test results for selected repos appear and refresh within ~60s.
- Logs show the majority of poll cycles returning `304 Not Modified` (free) when idle.
- Every item is correctly tagged Human/Bot and (for PRs) Feature/Security/Other, and each
  tag is filterable in one click.
- Slow CI runs are flagged with a reason and (if enabled) notified.
- Measured idle CPU is negligible on a typical laptop.

## 9. Phased Delivery (milestones)

| Phase | Outcome |
|-------|---------|
| 0 Skeleton | Workspace, crates, CI for 3 OSes, empty Iced window |
| 1 Auth + scope | PAT→keychain, validate, list & select orgs/repos |
| 2 Poller core | Conditional-request polling of open PRs; cheap refresh (304s) |
| 3 Enrichment | Reviews, comments, check-runs/test results in the feed |
| 4 Classification | Human/bot + layered feature/security, with correction |
| 5 CI timing + alerts | Baselines, slow-CI flag, native notifications (Linux) |
| 6 Filters + polish | Composable filter chips, adaptive cadence, settings UI |
| 7 Packaging | **Linux** installable artifacts (AppImage/.deb) |
| 8 (post-v1) | macOS/Windows packaging + notification backends |

> **Dev mode (R2a):** built mock-first against recorded GitHub fixtures with **no PAT**; live
> integration verification is a deferred pass run when a PAT is provided. See exec/MASTER §10.

## 10. Open Questions

- OQ1 GraphQL enrichment vs REST-only — decide when REST request volume warrants it (ARD).
- OQ2 Tray icon required, or notifications sufficient for v1?
- OQ3 Default poll cadence values (focused vs background) — confirm with user.
