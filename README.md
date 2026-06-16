# Alurtmee

A lightweight, low-impact **desktop dashboard for watching GitHub pull requests** — written in
Rust with [Iced](https://iced.rs). Alurtmee polls the repositories you care about, surfaces what
changed, classifies and enriches each PR, flags slow or failed CI, and otherwise stays out of your
way: it costs ~no CPU when idle and speaks to you through native desktop notifications.

> **Status:** Linux-only v1, in active development. The full pull-request pipeline (auth → poll →
> diff → enrich → classify → CI timing → filter/theme) is built and tested against recorded GitHub
> fixtures. Live verification against `api.github.com` and Linux packaging (AppImage/`.deb`) are the
> remaining milestones.

---

## Highlights

- **Two-pane dashboard** — a filtered list of open PRs on the left, full detail (reviews, comments,
  CI status) of the selected PR on the right.
- **Cheap, conditional polling** — ETag / `If-None-Match` conditional requests mean most poll
  cycles are free `304`s; the cadence adapts and backs off ~4× while the window is blurred.
- **Two-tier enrichment** — only PRs that actually changed are enriched (reviews, issue + review
  comments, check-runs / combined status → a single pass/fail/pending verdict).
- **Classification** — every PR is tagged **human / bot** and **feature / security / other** via a
  layered classifier (labels → title/branch prefix → changed paths → Dependabot), with the firing
  signal recorded and **user corrections** that persist and override.
- **CI/CD timing + alerts** — pulls GitHub Actions run timings, learns a rolling **p90 baseline**
  per workflow, flags runs that are too slow or failed, and fires **desktop notifications**.
- **Composable filters** — toggle-chips over source × category narrow the feed live.
- **Five dark themes** — Aurora, Velvet, Synthwave, Voltage, Ionix — switchable in-app and
  persisted. Colour is disciplined: green = good, gold = important (security is *highlighted*, not
  alarmed), red = bad only.
- **Privacy-first** — your token lives in the **OS keychain only**, never in SQLite, config files,
  or logs.

Try the look without a token:

```bash
ALURTMEE_DEMO=1 cargo run -p app
```

---

## How it works

Alurtmee is a Rust Cargo **workspace** of small, single-responsibility crates (ARD AD-7):

| Crate        | Responsibility                                                                       |
|--------------|--------------------------------------------------------------------------------------|
| `domain`     | Pure types + classifiers/stats — no I/O, exhaustively unit-tested                    |
| `gh-client`  | GitHub REST: auth, conditional requests, pagination, enrichment & Actions endpoints  |
| `store`      | Bundled SQLite cache (ETags, PR/enrichment cache, config) + OS-keychain wrapper       |
| `poller`     | Async scheduler + diff/change-detection; emits change events the UI subscribes to     |
| `app`        | The Iced UI + binary; wires poller → store → UI and dispatches notifications           |

Data flow: `poller` (tokio) → `gh-client` fetch → `store` persist + diff → emit events → Iced
`subscription` → UI updates (only changed widgets redraw) → optional desktop notification.

The UI is built with **Iced 0.13** (Elm architecture, `wgpu`-rendered), chosen for its
retained-mode, redraw-on-event model — an idle dashboard does no work between poll cycles.

---

## Getting started

### Prerequisites (Linux)

- **Rust** (stable; the workspace pins a recent toolchain via `rust-toolchain.toml`) and **Cargo**.
- A desktop session with **GPU/Wayland or X11** (Iced renders via `wgpu`).
- A running **Secret Service** (e.g. `gnome-keyring`) for token storage, over a D-Bus session.
- A **notification daemon** (most desktops have one) for CI alerts.

System libraries (Debian/Ubuntu names):

```bash
sudo apt-get install -y \
  libxkbcommon-dev libwayland-dev libxkbcommon-x11-dev libx11-dev \
  libgl1-mesa-dev mesa-vulkan-drivers \
  gnome-keyring dbus-x11
```

> macOS and Windows are explicitly **post-v1** (platform seams — keychain, notifications, packaging,
> windowing — are kept behind traits/`cfg` so adding them later is additive, not a rewrite).

### Build & run

```bash
# build everything
cargo build --workspace

# run the app (binary is `alurtmee`)
cargo run -p app

# demo mode — sample data, no token or network needed
ALURTMEE_DEMO=1 cargo run -p app
```

### Using it

1. Open **⚙ Settings**, paste a **fine-grained GitHub personal access token**, and click
   **Validate**. The token is stored in your OS keychain.
2. Pick the repositories you want to watch — your selection is persisted.
3. Back on the feed, Alurtmee starts polling: PRs appear, get enriched and classified, and CI
   alerts surface as you go. Pick a theme from the top-bar dropdown.

| Environment variable        | Purpose                                                                 |
|-----------------------------|-------------------------------------------------------------------------|
| `ALURTMEE_DEMO`             | If set, pre-populate the dashboard with sample data (no token/network).  |
| `ALURTMEE_GITHUB_BASE_URL`  | Override the GitHub REST base URL (default `https://api.github.com`).     |
| `RUST_LOG`                  | Tracing filter (e.g. `RUST_LOG=info`).                                    |

State lives under your platform data directory (`directories` crate): a bundled SQLite database for
the cache/config and selection. The token is in the OS keychain only.

---

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- **Testing strategy:** pure `domain` logic is table-tested; HTTP and cross-crate behaviour is
  verified against a local [`wiremock`](https://docs.rs/wiremock) server replaying recorded GitHub
  payloads; the OS keychain and desktop notifications are exercised live against the session D-Bus.
- **CI** (`.github/workflows/ci.yml`, Linux): `fmt --check`, `clippy -D warnings`, build, the full
  test suite (under an unlocked `gnome-keyring` D-Bus session), a headless `xvfb` window smoke test,
  and a coverage report.
- **Conventions:** typed `thiserror` errors in libraries / `anyhow` at the binary; one type per
  file; Conventional Commits.

---

## Roadmap

- ✅ Workspace, Linux CI, runnable window
- ✅ Auth + scope (PAT → keychain, repo selection)
- ✅ Conditional polling core (ETag/304, diff, adaptive cadence)
- ✅ Enrichment (reviews, comments, check-runs)
- ✅ Classification (human/bot, feature/security, corrections)
- ✅ CI/CD timing + slow-CI alerts + notifications
- ✅ Filters + dark theming (the dashboard you see today)
- ⬜ **Live verification** against real `api.github.com` (PAT-gated)
- ⬜ **Packaging** — Linux AppImage + `.deb` + release CI
- ⬜ macOS / Windows backends (post-v1)

---

## Privacy & security

Alurtmee reads from GitHub on your behalf and stores **only non-secret data** locally. Your access
token is written exclusively to the **OS keychain** — it is never persisted to SQLite, written to
config files, included in logs, or shown unmasked in the UI. Notification bodies carry no secrets.

---

## License

Licensed under the **Apache License, Version 2.0** — see [LICENSE](LICENSE).
