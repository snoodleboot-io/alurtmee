# Contributing to Alurtmee

Thanks for helping out! Bug reports, ideas, and PRs are all welcome.

## Getting set up

Alurtmee is a Rust (Iced) desktop app, Linux for now. You'll need [Rust](https://rustup.rs) and a
few system libraries:

```bash
# Debian / Ubuntu
sudo apt-get install -y \
  libxkbcommon-dev libwayland-dev libxkbcommon-x11-dev libx11-dev \
  libgl1-mesa-dev mesa-vulkan-drivers libdbus-1-dev \
  gnome-keyring dbus-x11

cargo run -p app                 # run it
ALURTMEE_DEMO=1 cargo run -p app # run with sample data, no token/network
```

`libdbus-1-dev` is needed because the keychain backend builds against the C `libdbus`.

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

You don't tag releases by hand. The version lives in the [`VERSION`](VERSION) file (semver), and a
PR's label decides the bump:

| Label | Bump | Example |
|-------|------|---------|
| `release` | minor | `0.3.0` → `0.4.0` |
| `release:major` | major | `0.4.0` → `1.0.0` |
| `release:patch` | patch | `0.4.0` → `0.4.1` |

When a release-labelled PR gets the label, CI commits the version bump to that PR's branch; merging
it then builds the `.deb` + AppImage, writes `SHA256SUMS`, attaches a build-provenance attestation,
and publishes a [GitHub Release](https://github.com/snoodleboot-io/alurtmee/releases). A PR with no
release label merges without cutting a release. See [`CHANGELOG.md`](CHANGELOG.md) for history.

## Reporting bugs

Open an [issue](https://github.com/snoodleboot-io/alurtmee/issues) with what you did, what you
expected, and what happened (with `RUST_LOG=info` output if relevant).
