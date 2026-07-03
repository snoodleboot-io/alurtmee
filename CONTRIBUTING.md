# Contributing to Alurtmee

Thanks for helping out! Bug reports, ideas, and PRs are all welcome.

## Getting set up

Alurtmee is a Rust (Iced) desktop app. Linux is the released target; macOS has a dev build
(unsigned; see below). You'll need [Rust](https://rustup.rs):

```bash
cargo run -p app                 # run it
ALURTMEE_DEMO=1 cargo run -p app # run with sample data, no token/network
```

On **Debian / Ubuntu** install the system libraries first:

```bash
sudo apt-get install -y \
  libxkbcommon-dev libwayland-dev libxkbcommon-x11-dev libx11-dev \
  libgl1-mesa-dev mesa-vulkan-drivers libdbus-1-dev \
  gnome-keyring dbus-x11
```

On **macOS** no extra packages are needed. Every OS-specific choice (keychain backend,
notification backend, open-in-browser command) is funnelled through `crates/app/src/platform.rs`
plus the target-conditional `keyring` features in `crates/store/Cargo.toml` — add a platform there,
not with `cfg` scattered across the code. The macOS binary is unsigned; a `.app`/`.dmg` with signing
+ notarization is a later ticket.

## Workspace layout

It's a Cargo workspace. Each crate has one job:

| Crate | Responsibility |
|-------|----------------|
| `domain` | Pure types + logic (classification, filters, diffing). No I/O. |
| `gh-client` | GitHub REST access (auth, pagination, ETags, rate limits). |
| `store` | Local persistence — SQLite (cache/config) + the OS keychain (token). |
| `poller` | The background poll loop; emits change events. |
| `app` | The Iced UI (`alurtmee` binary). |

## Before you push

CI runs these on every PR; run them locally first:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Conventions

- **Branch from `main`** — it's protected, so all changes land via PR. Don't commit to `main`.
- **[Conventional Commits](https://www.conventionalcommits.org/)** for messages
  (`feat:`, `fix:`, `refactor:`, `ci:`, `docs:`, `chore:`).
- `domain` keeps **one type per file**; libraries use `thiserror` for errors.
- Keep new code in the style of its neighbours.

## How releases work

You don't tag releases by hand, and the version is **not stored in the repo**. At release time CI
takes the latest published release and applies the bump your PR's label asks for:

| Label | Bump | Example (latest `v0.3.0`) |
|-------|------|---------------------------|
| `release` | minor | `v0.4.0` |
| `release:major` | major | `v1.0.0` |
| `release:patch` | patch | `v0.3.1` |

Just add one of those labels to your PR and merge it. The `release` workflow then derives the next
version from the latest GitHub Release, stamps it into `Cargo.toml` for the build, packages the
`.deb` + AppImage + macOS `.dmg`, writes `SHA256SUMS`, attaches a build-provenance attestation, and
publishes a [GitHub Release](https://github.com/snoodleboot-io/alurtmee/releases) tagged `vX.Y.Z`.
A PR with no release label merges without cutting a release.

Because the version comes from the releases (not a committed file), there is no bump commit to race
with the merge, and CI refuses to re-publish a version that already exists. To cut a release without
a PR — or to redo one — run the **release** workflow manually (`workflow_dispatch`) and pick the
bump. `Cargo.toml` carries a `0.0.0-dev` placeholder for local builds only. See
[`CHANGELOG.md`](CHANGELOG.md) for history.

## Reporting bugs

Open an [issue](https://github.com/snoodleboot-io/alurtmee/issues) with what you did, what you
expected, and what happened (with `RUST_LOG=info` output if relevant).
