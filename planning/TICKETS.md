# Ticket Registry (local scheme: ALRT-#)

This project has no external issue tracker. We use a lightweight local scheme, **ALRT-#**,
as the source of "real" ticket IDs required by the branch-naming convention. Increment the
counter for each new ticket; record it here before creating its branch.

**Next free ID:** ALRT-8

| Ticket | Title | Branch | Status |
|--------|-------|--------|--------|
| ALRT-1 | Design & plan Alurtmee (GitHub poller desktop app) | `feat/ALRT-1-github-poller-design` | Merged |
| ALRT-2 | Phase 0 — workspace scaffold, Linux CI, tracing, empty Iced window | `feat/ALRT-2-phase-0-scaffold` | Done |
| ALRT-3 | Phase 1 — Auth + Scope (PAT→keychain, validate, list orgs/repos, persist selection) | `feat/ALRT-3-phase-1-auth-scope` | Merged |
| ALRT-4 | Phase 2 — Poller core (conditional requests, ETag/304, diff, adaptive cadence, PR list) | `feat/ALRT-4-phase-2-poller-core` | Merged |
| ALRT-5 | Phase 3 — Enrichment (reviews, comments, check-runs/status → PR detail; enrich only on change) | `feat/ALRT-5-phase-3-enrichment` | Merged |
| ALRT-6 | Demo-seed UI mode (ALURTMEE_DEMO=1) — sample PRs + enrichment for manual UI review (no PAT) | `feat/ALRT-6-demo-seed-ui` | Merged |
| ALRT-7 | Phase 4 — Classification (human/bot + layered feature/security; corrections; per-repo config) | `feat/ALRT-7-phase-4-classification` | Done |
