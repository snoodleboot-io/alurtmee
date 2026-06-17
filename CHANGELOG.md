# Changelog

All notable changes to Alurtmee are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-06-17

First release: a calm Linux desktop app for keeping an eye on your GitHub pull requests.

### Added

- **Two-pane dashboard** — a filtered master list of open PRs with a detail pane (reviews, comments,
  CI status), and a CI-alerts strip.
- **Multiple personal access tokens**, each with a user-chosen label. Repos seen across tokens are
  de-duplicated, and every watched repo is polled by exactly one token, so PRs never appear twice.
  Org/collaborator access wins over a personal account when assigning a shared repo. ([#1])
- **Rename a token** in Settings; the secret moves to the new keychain entry. ([#1])
- **Session restore** — tokens (OS keychain) and repo selection persist, so launches reconnect and
  resume polling with no re-entry.
- **Human/bot + feature/security classification** with one-click corrections that stick.
- **Desktop notifications** the moment CI fails or a workflow runs slower than its usual time.
- **Six hand-built dark themes** (Nebula, Aurora, Velvet, Synthwave, Voltage, Ionix), switchable in
  Settings.
- **Startup animation** that plays and fades into the populated feed. ([#1])
- **Adaptive, low-cost polling** — conditional requests, focus/blur back-off, jitter.
- **Release pipeline** — version-file-driven semantic versioning, label-gated releases
  (`release` / `release:major` / `release:patch`), packaged `.deb` + AppImage, `SHA256SUMS`, and a
  signed build-provenance attestation, published to GitHub Releases. ([#2])

### Fixed

- **Polling could wedge silently** on databases from early builds: a `pull_requests` schema mismatch
  made every poll fail, and stale ETags then kept repos returning `304` against an empty cache.
  Databases now self-repair on launch (schema backfill + ETag reset), the ETag is only saved after a
  successful cache write, and the feed hydrates from the cache so a restart shows every open PR
  immediately. ([#1])

### Security

- The GitHub token lives only in the OS keychain (Secret Service) — never in the database, config,
  or logs. Release artifacts ship with checksums and a verifiable build-provenance attestation.

[0.1.0]: https://github.com/snoodleboot-io/alurtmee/releases/tag/v0.1.0
[#1]: https://github.com/snoodleboot-io/alurtmee/pull/1
[#2]: https://github.com/snoodleboot-io/alurtmee/pull/2
