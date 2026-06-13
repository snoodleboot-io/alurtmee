# Execution Plan — MASTER (ALRT-1, Alurtmee)

**Status:** Draft (awaiting approval — no lane starts until approved)
**Scope:** Shared spine for the 8 phase execution plans (`PHASE-0`…`PHASE-7`).
**Created:** 2026-06-13
**Related:** [PRD](../ALRT-1-PRD.md) · [ARD](../ALRT-1-ARD.md)

This master holds everything common to all phases. Each phase doc references it and adds only
phase-specific manifest, execution map, subagents, and verification.

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
  Unverifiable integration = blocker.
- **Tests non-negotiable**: ATDD at acceptance, TDD at unit+integration; they must **run and
  pass live**, exercise edge/failure paths.
- **No stubs** (per §3.4).
- **Understand before applying** (per §3.6).

## 6. Global gap & blocker report (Step 9, cross-phase)

| # | Gap / blocker | Phase(s) | Owner | Fallback / resolution |
|---|---|---|---|---|
| B1 | **Fine-grained GitHub PAT** — cannot be minted by pipeline | 1–7 | you | You create + paste once at Phase-1 gate; stored in keychain. Hard blocker until provided. |
| B2 | **Representative GitHub fixtures** (PRs incl. bot/security, comments, CI runs) | 2–6 | you + devops | Designate a sandbox repo you own; else use named public repos read-only. Flag if absent. |
| B3 | **Linux Secret Service** for `keyring` | 1+ | devops | Start `dbus` + `gnome-keyring-daemon` headless; CI fallback = `keyring` mock/file backend for unit tests only (integration still needs real). |
| B4 | **Linux notification daemon** for `notify-rust` | 5 | devops | Start dbus + a notif daemon; headless CI = dbus-mock assertion + 1 real-desktop manual-equivalent run owned by pipeline. |
| B5 | **Headless GPU/display** for Iced (wgpu) | 0,6,7 | devops | `xvfb` + llvmpipe/software adapter for CI smoke; document. |
| B6 | **Apple Developer ID + Windows signing cert** | 7 | you | Notarization/signing blocked until certs provided; pipeline still produces *unsigned* artifacts and flags the gap. |
| B7 | **Missing test conventions** (`rust.md` Testing = TODO) | 0+ | test-agent | §7 defines them as a Phase-0 deliverable for your ratification. |
| B8 | **Empty `core-conventions` TODOs** (error pattern, commit style, repo structure) | 0 | plan/architect | Phase-0 fills them from ARD + Rust norms; flagged for ratification. |

## 7. Test strategy framework (Step 7) — *fills the `rust.md` Testing TODO (provisional)*

- **Unit (TDD):** `#[cfg(test)] mod tests` co-located in each module. Pure `domain` logic gets
  exhaustive table-driven tests. HTTP-touching unit tests use **`wiremock`** (a local mock
  server) — allowed at the *unit* level only.
- **Integration:** `crate/tests/*.rs`. Cross-crate behavior. HTTP integration tests run against
  **real `api.github.com`** (gated on B1/B2) — this is the integration-verification evidence,
  not a mock.
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
test+lint+coverage suite live → enforcement-agent verifies conventions + no-stub +
understanding-demonstrated → security-agent reviews boundary-touching phases → **gate passes
only if all green**, then the phase's exit criteria are confirmed and the session file updated.
