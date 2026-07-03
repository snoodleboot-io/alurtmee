# ALRT-26 — macOS Release Asset + Site (PRD)

**Status:** In Review  
**Branch:** `feat/ALRT-26-macos-release`  
**Depends on:** [ALRT-25](../current/ALRT-25-PRD.md) (macOS dev build — merged, #13).

## 1. Problem
After ALRT-25 the app builds/tests/runs on macOS, but the only macOS artifact is a **CI artifact**:
login-gated, expiring, and a bare binary. The public downloads site still says "Linux today". The two
gaps: **(1)** no public macOS release asset, **(2)** the site doesn't advertise macOS.

## 2. Goal & non-goals
**Goal:** the release pipeline publishes an **unsigned universal `.dmg`** as a GitHub Release asset
(public, permanent, no login), and the downloads site advertises + renders it.

**In scope**
- macOS job in `release.yml`: universal binary → `Alurtmee.app` → `.dmg`, folded into the existing
  release (SHA256SUMS + provenance attestation + the published Release).
- `packaging/macos/`: `Info.plist` + `make-dmg.sh` (`.app` assembly, `.icns` from `assets/logo.png`).
- Site: `.dmg` asset classifier in `downloads.js`; copy flipped to advertise macOS.

**Out of scope (deferred)**
- **Code signing + notarization** (Apple Developer account). The `.dmg` is unsigned; first launch needs
  right-click → Open. Tracked as a later ticket.
- Windows.

## 3. Decisions
| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Distribute an unsigned `.dmg` (universal `.app` inside, + drag-to-Applications symlink) | Standard mac UX; unsigned is the chosen posture (ALRT-25 D2). |
| D2 | Add a `macos` job that `release` **needs**, not a matrix | Packaging steps are entirely OS-specific; a shared matrix would be mostly `if:` noise. The `.dmg` is built on `macos-14`, handed to the Linux `release` job as an artifact for one unified Release + SHA256SUMS + attestation. |
| D3 | Build `.icns` on the runner from `assets/logo.png` via `sips`+`iconutil` | Those tools are macOS-only; can't pre-generate on Linux. |
| D4 | Packaging logic in `packaging/macos/make-dmg.sh`, not inline YAML | Keeps `release.yml` readable and the script reusable/inspectable (DRY). |

## 4. Work
### 4.1 `packaging/macos/Info.plist`
Bundle metadata: `CFBundleIdentifier=com.snoodleboot.Alurtmee`, name `Alurtmee`, executable `alurtmee`,
icon `AppIcon`, `__VERSION__` placeholder (CI substitutes), `LSMinimumSystemVersion=11.0`,
`NSHighResolutionCapable=true`.

### 4.2 `packaging/macos/make-dmg.sh`
`(version, universal-binary-path, out-dir) →` assembles `Alurtmee.app` (Info.plist with version,
universal binary in `Contents/MacOS/`, `.icns` in `Contents/Resources/`), stages it beside an
`/Applications` symlink, and `hdiutil create`s `Alurtmee-<version>-universal.dmg`.

### 4.3 `release.yml`
- New `macos` job (`macos-14`, same checkout ref + `if:` guard as `release`): build both Apple
  targets, `lipo` a universal binary, run `make-dmg.sh`, upload the `.dmg` artifact.
- `release` job gains `needs: macos`; downloads the `.dmg` into `dist/`; SHA256SUMS, attestation, and
  the published `files:` all include `dist/*.dmg`.

### 4.4 Site
- `downloads.js`: classify `.dmg` → "macOS · Universal (Intel + Apple Silicon)"; add a mac icon; slot
  it into `ASSET_ORDER` after the Linux assets.
- `index.html` / `downloads.html`: replace "Linux today · macOS & Windows planned" →
  "Linux & macOS · Windows planned"; update the downloads lede, meta descriptions, and install blurb
  (open the `.dmg`, drag to Applications, unsigned → right-click → Open).

## 5. Acceptance
1. A release build produces `Alurtmee-<version>-universal.dmg`; `lipo -info` on the embedded binary
   shows `x86_64` + `arm64`.
2. The `.dmg` appears in the published GitHub Release, in SHA256SUMS, and carries a provenance
   attestation.
3. The downloads page renders the `.dmg` as a "macOS" button (not a generic file).
4. Site copy advertises macOS; Linux `.deb`/AppImage flow unchanged.
5. Linux release path is unregressed.

## 6. Go-live note
This ticket wires the pipeline; the `.dmg` becomes publicly downloadable only when a **release is cut**
(a `release`-labelled PR merge, or a `workflow_dispatch` re-run of `release`). The current live release
is v0.1.0 (Linux-only). Cutting the next release — or re-running `release` — publishes the macOS asset.

## 7. Risks
- **R1 — `.icns` from a non-1024 source.** `assets/logo.png` is high-res; `sips` downscales cleanly. If
  it's < 1024², the largest slice upscales (slightly soft) — acceptable for an unsigned dev release.
- **R2 — unsigned `.dmg` UX.** Gatekeeper blocks first launch; documented in the site install blurb.
- **R3 — `make-dmg.sh` only runs in CI** (macOS-only tools). Guard with `shellcheck` locally; first real
  execution is the macOS release job.
