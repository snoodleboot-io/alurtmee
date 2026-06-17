<p align="center">
  <img src="assets/logo.png" alt="Alurtmee" width="240">
</p>

<h1 align="center">Alurtmee</h1>

<p align="center"><em>A calm desktop app for keeping an eye on your GitHub pull requests.</em></p>

Point it at the repositories you care about and Alurtmee shows you — at a glance — which PRs are
open, who's behind them, whether CI is passing, and what needs your attention. When a build fails or
a workflow drags, it taps you on the shoulder with a desktop notification. The rest of the time it
stays quiet and out of the way.

> Try it with no setup: `ALURTMEE_DEMO=1 cargo run -p app` opens the dashboard filled with sample
> data — no GitHub token, no network.

## What it does for you

- **All your pull requests in one window** — open PRs across the repos you pick, each with its
  reviews, comment threads, and test status.
- **Know what you're looking at** — every PR is tagged **human or bot** and **feature or security**,
  with security changes highlighted. Got one wrong? Fix it in one click and it sticks.
- **Catch trouble early** — a desktop notification the moment CI **fails** or a workflow runs
  **slower than its usual time**, so you're not the last to find out.
- **Cut the noise** — one-click filter chips ("just bots", "just security", …) narrow the feed.
- **Make it yours** — six hand-built dark themes (Nebula, Aurora, Velvet, Synthwave, Voltage,
  Ionix), switchable on the fly.
- **Light and private** — easy on CPU/battery (it polls less when the window isn't focused), and
  your access token is stored **only in your system keychain** — never on disk, never in a log.

## Installing

Alurtmee runs on **Linux** today (macOS and Windows are planned). Grab a packaged build from the
[**Releases**](https://github.com/snoodleboot-io/alurtmee/releases/latest) page, or build from source.

It renders with your GPU and stores your token in the system keychain, so you need GPU drivers
(`mesa-vulkan-drivers`) and a Secret Service daemon (`gnome-keyring`) present either way.

### Option A — AppImage (no install)

```bash
# from the latest release; or download via the Releases page
curl -L -o Alurtmee.AppImage \
  https://github.com/snoodleboot-io/alurtmee/releases/latest/download/Alurtmee-x86_64.AppImage
chmod +x Alurtmee.AppImage
./Alurtmee.AppImage
```

### Option B — Debian / Ubuntu `.deb`

```bash
sudo apt install ./alurtmee_*_amd64.deb   # pulls in its runtime dependencies
alurtmee
```

### Verify your download

Every release ships a `SHA256SUMS` file and a signed build-provenance attestation, so you can
confirm the binary is intact and was built by this repo's CI:

```bash
sha256sum -c SHA256SUMS                                   # integrity
gh attestation verify Alurtmee.AppImage \
  --repo snoodleboot-io/alurtmee                          # authenticity (needs the gh CLI)
```

### Option C — build from source

You'll need [Rust](https://rustup.rs) and a few system libraries:

```bash
# Debian / Ubuntu
sudo apt-get install -y \
  libxkbcommon-dev libwayland-dev libxkbcommon-x11-dev libx11-dev \
  libgl1-mesa-dev mesa-vulkan-drivers \
  gnome-keyring dbus-x11

git clone https://github.com/snoodleboot-io/alurtmee.git
cd alurtmee
cargo run -p app
```

## Getting connected

1. Click **⚙ Settings** in the top bar.
2. Create a GitHub **personal access token**
   (GitHub → *Settings → Developer settings → Personal access tokens*) with **read** access to the
   repositories you want to watch. Under **GitHub tokens**, give it a short **label**, paste the
   token, and click **Add token**. It's saved straight to your system keychain.
3. **Add as many tokens as you need** — e.g. a personal one and a work/org one. Each is labelled and
   stored separately; the repositories they can see are combined into one list (a repo visible to
   more than one token appears once).
4. Tick the repositories you'd like to keep an eye on, then head back to the feed and it fills in.

You only do this once: your tokens stay in the system keychain and your repo selection in the local
database, so the next time you launch Alurtmee it reconnects and resumes polling automatically — no
re-entry needed. Each watched repo is polled by a single token, so its PRs never show up twice. (If a
token is later revoked, its row shows the error so you can re-add or remove it.)

## Using it

- **Left pane:** your open PRs. Click one to see its full detail on the right. A small **dot** shows
  CI status (green = passing, red = failing, gold = running), and security PRs get a **gold edge**.
- **Filter chips** at the top of the list narrow it by source and kind.
- **CI alerts** appear in a strip across the top, and — unless you switch them off with the
  **Notifications** toggle — as desktop notifications.
- **Theme picker** lives in **⚙ Settings** — switch between the six looks; your choice is remembered.
- **Wrong tag?** On a PR's detail, click **→ feature** or **→ security** to correct its category.
  The correction persists and is reused next time.

## Settings & your data

- **Your token** lives only in the OS keychain (via the Secret Service). It is never written to the
  database, config, or logs.
- **Everything else** — the cache, your repo selection, your theme and preferences — is kept in a
  small local database under your user data directory.

A couple of environment variables you might use:

| Variable        | What it does                                                        |
|-----------------|---------------------------------------------------------------------|
| `ALURTMEE_DEMO` | Launch with sample data and no network (great for a first look).    |
| `RUST_LOG`      | Turn on logging, e.g. `RUST_LOG=info`.                               |

## Troubleshooting

- **“Could not store token in keychain.”** Alurtmee needs a running Secret Service to hold your
  token — install and start one (e.g. `gnome-keyring`), then try again.
- **No notifications.** You need a desktop notification daemon (most desktops ship one), and the
  **Notifications** toggle must be on.
- **The window won't open.** Alurtmee renders with your GPU and needs a graphical session
  (Wayland or X11).

## Contributing

Bug reports and ideas are welcome via [issues](https://github.com/snoodleboot-io/alurtmee/issues).
It's a Rust workspace — `cargo test --workspace` runs the suite.

**Releases are label-driven.** The version lives in the [`VERSION`](VERSION) file (semantic
versioning). Add a label to a PR and the version is bumped automatically on its own branch; merging
it then builds the `.deb` + AppImage and publishes a GitHub Release — no manual tags.

| Label | Bump | Example |
|-------|------|---------|
| `release` | minor | `0.3.0` → `0.4.0` |
| `release:major` | major | `0.4.0` → `1.0.0` |
| `release:patch` | patch | `0.4.0` → `0.4.1` |

A PR with no release label merges without cutting a release. `main` is protected (changes land via
PR), so the bump is committed to the PR branch — never pushed to `main` directly.

## License

Apache-2.0 — see [LICENSE](LICENSE).
