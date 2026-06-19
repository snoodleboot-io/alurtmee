# Execution Plan — MASTER (ALRT-1, Alurtmee)

**Status:** Draft (awaiting approval — no lane starts until approved)
**Scope:** Shared spine for the 8 phase execution plans (`PHASE-0`…`PHASE-7`).
**Created:** 2026-06-13
**Related:** [PRD](../ALRT-1-PRD.md) · [ARD](../ALRT-1-ARD.md)

This master holds everything common to all phases. Each phase doc references it and adds only
phase-specific manifest, execution map, subagents, and verification.

---

## Revisions

**R2 (2026-06-13) — two approved scope changes that override conflicting text below:**

- **R2a — Mock-first development; live integration verification deferred.** Per user choice, we
  develop and test against **`wiremock` + recorded real fixtures**, with **no GitHub PAT yet**.
  This is a deliberate, approved **deviation from §5 "integrations real & verified"**: each phase
  reaches **"logic-complete / contract-verified against recorded fixtures"** now; the
  **live integration-verification step is deferred** to a later *Integration Verification pass*
  that runs once a PAT is supplied (see §10). No phase is "integration-verified" in the interim,
  and that is stated explicitly at each gate — not hidden. The env-setup gate's PAT/network items
  are replaced by **mock-server + fixtures readiness** until then.
- **R2b — Linux-only v1; portability preserved.** Windows/macOS are **post-v1**. v1 CI,
  packaging (AppImage/.deb), and notifications target Linux only. Platform-specific surfaces
  (keychain, notifications, packaging, windowing) stay **behind traits / cfg seams** so adding
  mac/win later is additive, not a rewrite. Phase 7 is Linux-only; a future Phase 8 covers mac/win.

R1 = original issue of MASTER + PHASE-0..7.

---

## 0. Execution model (how "genuinely parallel" works here)

The `.claude/agents/*` files are **prompt-personas, not running daemons**. In this harness,
parallelism is realized by the **orchestrator** (orchestrator-agent persona, driven by Claude
Code) **spawning subagents via the Agent/Workflow tools**, each subagent adopting a persona +
the loaded conventions + a scoped task + its shared interfaces. Lanes with no data dependency
are spawned in the **same batch** so they run concurrently; lanes with dependencies are gated.
This is real concurrency, orchestrator-driven — not 24 self-running services. **(Confirmed with
user.)**

## 1. Conventions loaded (Step 1)

| Convention | Location | Loaded |
|---|---|---|
| Startup/branch/session, scope discipline, "flag new deps", terminal rules | [.claude/conventions/core/general.md](../../../.claude/conventions/core/general.md) | ✅ |
| Rust: `thiserror`/`anyhow`, `Result`, traits-for-abstraction, derives, snake_case, one-type-per-file, SOLID | [.claude/conventions/languages/rust.md](../../../.claude/conventions/languages/rust.md) | ✅ |
| Feature workflow (Plan→Confirm→Implement→Follow-up) | [.claude/workflows/feature.md](../../../.claude/workflows/feature.md) | ✅ |
| Project decisions | [ARD](../ALRT-1-ARD.md) / [PRD](../ALRT-1-PRD.md) | ✅ |

**Convention gaps / ambiguities flagged:**
- `core-conventions` has unfilled `TODO`s: Repository Structure, Error-Handling *pattern*,
  Database, Commit style, PR-size, Deployment.
- `rust.md` **Testing section = `TODO`** → there is **no established test pattern** to inherit.
  This plan therefore **defines one** (§7) as a Phase-0 deliverable; treat it as provisional
  until you ratify it.
- Conventions assume an external ticket tracker; we use the local **ALRT-#** scheme
  ([TICKETS.md](../../TICKETS.md)).

## 2. Agent roster → role map (Step 2)

| Pipeline role | Persona | Notes |
|---|---|---|
| PM / architect | plan-agent, architect-agent, product-agent | PRD/ARD authored; architect reviews per-phase design |
| code | code-agent | all production code |
| TDD (unit/integration) | test-agent (TDD hat) | |
| **ATDD (acceptance)** | test-agent (ATDD hat) | ⚠️ no distinct ATDD agent → run as a separate subagent instance with acceptance-only scope |
| verify | review-agent + `/verify` skill | |
| enforce | enforcement-agent | gate checkpoint |
| security | security-agent + `/security-review` | |
| debug | debug-agent | owns retry loop |
| **environment setup** | devops-agent | ⚠️ no dedicated env-readiness persona → devops-agent owns it |
| aggregator / coordinator | orchestrator-agent | spawns lanes, aggregates, gates |
| docs / changelog | document-agent | Phase 7 + per-phase session/PRD updates |

**Roster gaps:** ATDD and environment-setup have no purpose-built persona; covered by
double-hatting test-agent and assigning devops-agent respectively. No quality-loss expected; the
scopes are distinct enough to isolate per subagent.

## 3. Standing constraints (apply to every phase, verifiable at each gate)

1. No subagent begins until **you approve the relevant phase plan**.
2. **Env-setup subagent is a hard gate** — it must complete and pass health checks before any
   other lane in that phase is unblocked.
3. All work stays **within loaded conventions**; enforcement-agent verifies at the gate.
4. **No stubs / placeholders / deferred impl** in delivered output (`todo!()`,
   `unimplemented!()`, `NotImplementedError`, commented-out logic, placeholder returns = a gate
   failure).
5. **No agent instructs the human** to start a service / run a command / set up infra — the
   pipeline owns it via Bash. *Exception that is impossible to own:* secrets that only a human
   can mint (GitHub PAT, Apple/Windows signing certs) — surfaced as **blockers**, never stubbed.
6. **No pattern applied without demonstrated understanding** — every design-bearing subagent must
   state, in its output, *why* the pattern fits this context, or flag uncertainty for review.
7. **Material change** to roster/conventions/env/plan → **pause all lanes and re-present**.

## 4. Environment-as-gate principle (Step 4, generic)

Each phase's env-setup lane (devops-agent) must, before unblocking anything else:
identify every required service/daemon/watcher → **start or verify** each (never assume) →
confirm ports/health → start watchers → document start/verify/stop. "Laziness is not
acceptable": a needed service that won't start is an **immediate blocker**, not a silent skip.
Phase docs carry the concrete manifest.

## 5. Implementation standards (Step 5, binding)

- **Integrations real & verified** end-to-end; "compiles" / "unit test vs mock passes" ≠ verified.
  **R2a deviation:** during mock-first dev this is satisfied interim by **contract tests against
  recorded *real* fixtures** (captured from live GitHub once, checked into `tests/fixtures/`), and
  the true live verification is **deferred to the §10 Integration Verification pass** when a PAT
  exists. Each gate states "live verification: DEFERRED (R2a)".
- **Tests non-negotiable**: ATDD at acceptance, TDD at unit+integration; they must **run and pass**
  (against the mock server / fixtures while PAT-less), exercise edge/failure paths.
- **No stubs** (per §3.4) — fixtures are real recorded payloads, not invented stubs; production
  code paths are fully implemented (only the *remote* is replayed).
- **Understand before applying** (per §3.6).

## 6. Global gap & blocker report (Step 9, cross-phase)

| # | Gap / blocker | Phase(s) | Owner | Status under R2 | Resolution |
|---|---|---|---|---|---|
| B1 | **Fine-grained GitHub PAT** — cannot be minted by pipeline | 1–6 | you | **Deferred (R2a)** — not blocking dev | Dev proceeds mock-first; live integration verification (§10) waits for a PAT whenever you choose to provide one. |
| B2 | **Representative GitHub data** | 2–6 | devops | **Replaced (R2a)** | Use **recorded real fixtures** captured once into `tests/fixtures/` (incl. a 304 case and a changed case). Live sandbox deferred with B1. |
| B3 | **Linux Secret Service** for `keyring` | 1+ | devops | Active | Pipeline starts `dbus` + `gnome-keyring-daemon`; keychain tested with a **dummy token** (no real PAT needed). |
| B4 | **Linux notification daemon** for `notify-rust` | 5 | devops | Active (Linux only) | dbus + notif daemon; verified via dbus-mock + dev-desktop. Notifications need no PAT (fed by fixtures). |
| B5 | **Headless GPU/display** for Iced (wgpu) | 0,6,7 | devops | Active | `xvfb` + llvmpipe software adapter for CI smoke. |
| ~~B6~~ | ~~Apple/Windows signing certs~~ | ~~7~~ | — | **Out of v1 (R2b)** | macOS/Windows deferred to post-v1 Phase 8; no certs needed for Linux v1. |
| B7 | **Missing test conventions** (`rust.md` Testing = TODO) | 0+ | test-agent | Active | §7 defines them as a Phase-0 deliverable for your ratification. |
| B8 | **Empty `core-conventions` TODOs** | 0 | plan/architect | Active | Phase-0 fills from ARD + Rust norms; flagged for ratification. |

**Scope (R2b):** v1 = **Linux only**. mac/win packaging, notification backends, and CI runners
move to a future **Phase 8**; code keeps platform seams so that phase is additive.

## 7. Test strategy framework (Step 7) — *fills the `rust.md` Testing TODO (provisional)*

- **Unit (TDD):** `#[cfg(test)] mod tests` co-located in each module. Pure `domain` logic gets
  exhaustive table-driven tests. HTTP-touching unit tests use **`wiremock`** (a local mock
  server) — allowed at the *unit* level only.
- **Integration (R2a, two-stage):** `crate/tests/*.rs`. Cross-crate behavior. **Stage 1 (now):**
  run against **recorded real fixtures** replayed by `wiremock` — captured from live GitHub once
  and committed to `tests/fixtures/` (these are real payloads, not invented). **Stage 2
  (deferred):** the same tests re-run against **real `api.github.com`** in the §10 pass once a PAT
  exists — that is the true integration-verification evidence.
- **Fixture capture:** a one-time `xtask`/script records the needed responses (PR list incl. a
  304, reviews, comments, check-runs, actions runs) using *any* token at capture time, scrubs
  secrets, and commits them. Until then, agreed representative fixtures are authored from GitHub's
  documented schemas and replaced with recorded ones at first capture.
- **Acceptance (ATDD):** scenarios authored *before* code by the ATDD subagent, expressed as
  end-to-end checks (e.g. driving the poller + store and asserting observable outcomes; UI
  acceptance via headless smoke + state assertions).
- **Coverage:** `cargo-llvm-cov`; target **≥ 80% meaningful** on `domain`/`gh-client`/`store`/
  `poller` (UI exempt from line-target, covered by acceptance smokes). Coverage is evidence, not
  the goal.
- **Lint/format gates:** `cargo fmt --check`, `cargo clippy -- -D warnings` on every lane.
- ATDD scenarios and TDD tests are produced by **concurrent subagents** and reconciled at the
  aggregation step against this framework.

## 8. Debug & retry logic (Step 10, generic)

- **Owner:** debug-agent.
- **Surfacing:** failures appear at (a) a lane's own test run, (b) the **aggregation** step
  (orchestrator consistency check), or (c) the **enforcement** gate.
- **Retry scope ladder:** ① re-run the failing *subagent* with the failure context (max 2);
  ② if cross-cutting, re-run the whole *lane*; ③ if it implicates an interface contract, pause
  dependent lanes and reconcile at aggregation.
- **Escalate to you** when: a **blocker** from §6 is hit; 2 subagent + 1 lane retry all fail;
  a convention conflict has no in-bounds resolution; or a §3.6 understanding-gap can't be
  resolved without product input.

## 9. Per-phase gate (aggregation, generic)

orchestrator-agent collects lane outputs → checks interface consistency → runs the full
test+lint+coverage suite (against fixtures under R2a) → enforcement-agent verifies conventions +
no-stub + understanding-demonstrated → security-agent reviews boundary-touching phases → **gate
passes only if all green**, then the phase's exit criteria are confirmed and the session file
updated. Each gate explicitly records **"live integration verification: DEFERRED (R2a)"** for
GitHub-touching phases.

## 10. Integration Verification pass (R2a — deferred, PAT-gated)

A standalone pass, runnable at any time **once you provide a PAT**, that converts every deferred
"Stage 2" verification into real evidence — without re-doing feature work:

1. env-setup obtains the PAT (keychain) + confirms a (sandbox or public) repo with the needed data.
2. Re-run each GitHub-touching phase's **integration tests against `api.github.com`**: live auth
   (P1), live 304 + `X-RateLimit-Remaining` unchanged + change-detect (P2), three enrichment
   families (P3), classifier inputs `/pulls/{n}/files` + advisory (P4), Actions timing (P5).
3. Capture/refresh `tests/fixtures/` from the real responses so Stage-1 tests track reality.
4. Any divergence between fixtures and live = a finding routed to debug-agent (MASTER §8); if it
   implies an interface/semantic change, **pause + re-present** (§3.7).

Until this pass runs, the product is **logic-complete but not live-verified**, and we say so.
