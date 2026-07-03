# ALRT-27 — Tag-Derived Versioning (PRD)

**Status:** In Review  
**Branch:** `feat/ALRT-27-tag-derived-versioning`

## 1. Problem
The release version lived in a committed `VERSION` file, bumped by `version-bump.yml` which **pushed a
commit to the PR branch** when a `release*` label was added. Two failures:
1. **Race:** if the PR is merged before/at the same time as the bump commit lands (and with squash
   merges), the bump is lost — `main` keeps the old version and `release.yml` tries to **re-publish an
   existing tag**. This actually happened cutting the macOS release (v0.1.0 nearly got mutated).
2. **Embedded version:** the source of truth for "what version are we" sat in the repo, duplicated
   across `VERSION` + `Cargo.toml`, and had to be kept in sync.

## 2. Goal
The version is **derived from what is released**, not stored in the repo. No bump commit, no race.

## 3. Design
- **Delete** `VERSION` and `version-bump.yml`.
- **`Cargo.toml`** carries a non-authoritative `0.0.0-dev` placeholder (Cargo requires *some* version);
  local/dev builds report that. It is never the release version.
- **`release.yml`** gains a `version` job — the single source of truth:
  - base = latest published GitHub Release (`gh release view`, `vX.Y.Z`); none yet → `0.0.0`;
  - bump = the merged PR's label (`release`→minor, `release:patch`→patch, `release:major`→major), or
    the `workflow_dispatch` `bump` input;
  - **collision guard:** if the computed `vX.Y.Z` already exists, fail loudly instead of clobbering.
  - Emits `version` as a job output.
- **`macos` + `release`** jobs `needs: version`, read `needs.version.outputs.version`, and stamp it
  into `Cargo.toml` (`sed`, ephemeral in CI) before building so `cargo-deb` / `CARGO_PKG_VERSION` /
  Info.plist all match. The trigger guard lives only on `version`; the others inherit it via `needs`
  (DRY — one guard, one version computation shared by both).

### The product knows its own version
Deriving the version at release is pointless if the app can't report it. The version is now **baked
into the binary** and surfaced:
- `pub(crate) const APP_VERSION = env!("CARGO_PKG_VERSION")` — compile-time, so it reflects whatever
  the release workflow stamped (real `X.Y.Z` for releases; `0.0.0-dev` for local builds).
- `alurtmee --version` / `-V` prints it and exits before the UI starts.
- The Settings panel shows an `Alurtmee vX.Y.Z` footer, so the running product always displays which
  release it is.

## 4. Why this is better
- **No race:** nothing is committed at release time; the version is computed from immutable release
  history at the moment of publish.
- **No duplication / drift:** one derivation, no `VERSION`↔`Cargo.toml` sync.
- **Safe by construction:** the collision guard makes "silently re-publish an existing version"
  impossible — the exact bug that bit the macOS release.

## 5. Acceptance
1. `VERSION` and `version-bump.yml` are gone; nothing reads a committed version at release time.
2. A `release:patch` PR merged against latest `v0.1.0` publishes **v0.1.1**; `release` → v0.2.0;
   `release:major` → v1.0.0 (verified by unit-testing the compute logic).
3. Re-triggering against an existing version **fails** rather than mutating a release.
4. Published `.deb`/AppImage/`.dmg` all report the derived version.
5. `workflow_dispatch` can cut a release with a chosen bump and no PR.
6. The **product reports its own version**: `alurtmee --version` prints it, and the Settings footer
   shows it. Release builds show the stamped `X.Y.Z`; dev builds show `0.0.0-dev`.

## 6. Follow-up
- Optional: fully-automatic bump inferred from Conventional Commits (`feat`→minor, `fix`→patch,
  `BREAKING`→major) instead of an explicit label. Deferred — explicit labels are predictable and were
  not the problem.
