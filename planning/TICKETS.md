# Ticket Registry (local scheme: ALRT-#)

This project has no external issue tracker. We use a lightweight local scheme, **ALRT-#**,
as the source of "real" ticket IDs required by the branch-naming convention. Increment the
counter for each new ticket; record it here before creating its branch.

**Next free ID:** ALRT-22

| Ticket | Title | Branch | Status |
|--------|-------|--------|--------|
| ALRT-1 | Design & plan Alurtmee (GitHub poller desktop app) | `feat/ALRT-1-github-poller-design` | Merged |
| ALRT-2 | Phase 0 — workspace scaffold, Linux CI, tracing, empty Iced window | `feat/ALRT-2-phase-0-scaffold` | Done |
| ALRT-3 | Phase 1 — Auth + Scope (PAT→keychain, validate, list orgs/repos, persist selection) | `feat/ALRT-3-phase-1-auth-scope` | Merged |
| ALRT-4 | Phase 2 — Poller core (conditional requests, ETag/304, diff, adaptive cadence, PR list) | `feat/ALRT-4-phase-2-poller-core` | Merged |
| ALRT-5 | Phase 3 — Enrichment (reviews, comments, check-runs/status → PR detail; enrich only on change) | `feat/ALRT-5-phase-3-enrichment` | Merged |
| ALRT-6 | Demo-seed UI mode (ALURTMEE_DEMO=1) — sample PRs + enrichment for manual UI review (no PAT) | `feat/ALRT-6-demo-seed-ui` | Merged |
| ALRT-7 | Phase 4 — Classification (human/bot + layered feature/security; corrections; per-repo config) | `feat/ALRT-7-phase-4-classification` | Merged |
| ALRT-8 | Phase 5 — CI/CD timing + alerts (Actions runs, p75/p90 baseline, slow-flag, Linux notifications) | `feat/ALRT-8-phase-5-ci-timing` | Merged |
| ALRT-9 | Phase 6 — Filters + Polish (composable filter chips, focus/blur cadence, dark theming, empty/error states) | `feat/ALRT-9-phase-6-filters-polish` | Done |
| ALRT-12 | Brand logo asset + themes/UI polish (Nebula+Ionix, dual-neon, rounded skin-styled controls) | `feat/ALRT-12-logo-and-theme` | Merged |
| ALRT-13 | SOLID Stage 1 — DIP seams: `GhApi`/`PollStore` ports, generic `Poller<C,S>`, fake-driven unit tests | `feat/ALRT-13-testability-seams` | Merged |
| ALRT-14 | SOLID Stage 3 — split `app/main.rs` (theme registry, view/ helpers, per-pane views, update handlers) | `feat/ALRT-14-split-main` | Merged |
| ALRT-15 | SOLID Stage 2+4 — decompose `Store` into 5 concern modules; list-driven migrations; gh-client header DRY | `feat/ALRT-15-solid-store-ocp` | Merged |
| ALRT-16 | Restore session on launch — re-read keychained PAT in `boot`, re-validate, resume polling (no re-entry) | `feat/ALRT-16-restore-session` | Merged |
| ALRT-17 | Multiple labelled PATs + dedupe — keychain per-label tokens, aggregated/deduped repo list, one poller per token over disjoint repos | `feat/ALRT-17-multi-pat` | Merged |
| ALRT-18 | Ownership rule: org/collaborator access trumps personal account when assigning a shared repo to a poller | `feat/ALRT-18-org-trumps-personal` | Merged |
| ALRT-19 | Fix wedged polling (v6 backfill + v7 etag reset, etag-after-cache), capture repo owner type, rename tokens; feed hydration + classification persistence + startup animation | `feat/ALRT-19-token-rename-owner-type` | In Review |
| ALRT-20 | Release pipeline — VERSION file, label-gated semver (release/:major/:patch), auto-bump on PR branch, .deb+AppImage+SHA256SUMS+provenance, branch protection | `feat/ALRT-20-release-pipeline` | Merged |
| ALRT-21 | Add CHANGELOG.md + CONTRIBUTING.md (clickable PR links); slim README Contributing | `feat/ALRT-21-changelog-contributing` | In Review |
