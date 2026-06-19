<!-- path: prompticorn/prompts/agents/core/core-conventions-rust.md -->
{%- import 'macros/testing_sections.jinja2' as testing -%}
{%- import 'macros/coverage_targets.jinja2' as coverage -%}
# Core Conventions Rust

Language:             Rust 1.95 (stable, pinned via rust-toolchain.toml)
Runtime:              Native
Package Manager:      Cargo (workspace)
Linter:               Clippy (`-D warnings`)
Formatter:           rustfmt

### Naming Conventions

Files:               snake_case
Variables:          snake_case
Constants:          UPPER_SNAKE
Classes/Types:      PascalCase
Functions:          snake_case
Database tables:    snake_case
Environment vars:   UPPER_SNAKE_CASE always

## Rust-Specific Rules

### Error Handling
- Use `Result<T, E>` for fallible operations - never panic in library code
- Use `?` operator for error propagation
- Use `thiserror` or `anyhow` for error handling
- Wrap errors with context using `map_err` or `with_context`

### Ownership & Borrowing
- Follow ownership rules - no use-after-free, no data races
- Use lifetimes when references must outlive their referents
- Prefer borrowing over cloning where possible
- Use `Arc` for shared ownership, `Rc` for single-threaded

### Traits & Generics
- Use traits for abstraction, not concrete types
- Prefer trait bounds over generic parameters
- Implement `Default`, `Clone`, `Debug`, `Display`, `Serialize`, `Deserialize` where appropriate

### Testing

Test framework for this project (transcribed from [exec/MASTER §7](../../../planning/complete/exec/MASTER.md)):

- **Unit (TDD):** `#[cfg(test)] mod tests` co-located in each module. Pure `domain` logic gets
  exhaustive **table-driven** tests. HTTP-touching unit tests use **`wiremock`** (a local mock
  server) — allowed at the *unit* level only.
- **Integration (R2a, two-stage):** `crate/tests/*.rs`, exercising cross-crate behavior.
  - *Stage 1 (now):* run against **recorded real fixtures** replayed by **`wiremock`** —
    captured from live GitHub once and committed to `tests/fixtures/` (real payloads, not
    invented stubs).
  - *Stage 2 (deferred):* the same tests re-run against **real `api.github.com`** in the
    deferred Integration Verification pass once a **PAT** exists — that is the true
    integration-verification evidence (R2a).
- **Fixture capture:** a one-time `xtask`/script records the needed responses (PR list incl. a
  304, reviews, comments, check-runs, actions runs), scrubs secrets, and commits them. Until
  capture, representative fixtures are authored from GitHub's documented schemas and replaced
  with recorded ones at first capture.
- **Acceptance (ATDD):** scenarios authored **before** code, expressed as end-to-end checks
  (e.g. driving the poller + store and asserting observable outcomes). **UI acceptance via
  headless smoke** + state assertions.
- **Coverage:** **`cargo-llvm-cov`**; target **≥ 80% meaningful** on
  `domain` / `gh-client` / `store` / `poller`. **UI is exempt from the line target** (covered by
  acceptance smokes). Coverage is evidence, not the goal.
- **Lint/format gates (every lane):** `cargo fmt --check`, `cargo clippy -- -D warnings`.

**Ratified in Phase 0 (B7 closed).**
