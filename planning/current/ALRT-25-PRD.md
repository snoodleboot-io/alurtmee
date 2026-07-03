# ALRT-25 — macOS Dev Build (PRD)

**Status:** Planning  
**Branch:** `feat/ALRT-25-macos-build`  
**Author:** planning session, 2026-06-24  
**Depends on:** v0.1.0 (Linux release). Builds on the cross-platform seams designed in
[ALRT-1 ARD](../complete/ALRT-1-ARD.md).

## 1. Problem

Alurtmee v0.1.0 ships Linux-only (`.deb` + AppImage). The codebase was written with macOS in
mind — the `Notifier` trait explicitly notes "macOS/Windows backends are post-v1 and swappable" —
but three platform seams are still hard-wired to Linux and there is no macOS CI. We want Alurtmee
to **compile, test, and run on macOS**.

## 2. Goal & non-goals

**Goal:** A green macOS build producing a runnable **universal binary** (x86_64 + arm64), with the
platform seams ported so the app behaves natively on macOS. Distribution-quality polish is out of
scope for this ticket.

**In scope**
- Port the three Linux-specific seams (keychain backend, notifications, open-in-browser).
- Universal binary (`x86_64-apple-darwin` + `aarch64-apple-darwin`, `lipo`-merged).
- macOS CI job (build + test) added to `ci.yml`.
- Docs: note macOS dev-build steps + Gatekeeper caveat.

**Out of scope (explicitly deferred)**
- **Code signing & notarization** → deferred to a later ticket (decision: ship unsigned for now).
- **`.app` bundle + `.dmg` packaging** and a macOS release pipeline (`release.yml` untouched).
- Windows support.
- Homebrew / Mac App Store distribution.

## 3. Decisions (this ticket)

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Scope = **dev build first**, not a distributable | Unblocked today, $0, no external dependencies. |
| D2 | **Unsigned** — no Apple Developer account yet | Signing/notarization deferred; users right-click→Open past Gatekeeper. |
| D3 | **Universal binary** (Intel + Apple Silicon) | Broadest Mac coverage; needs no signing, composes with D2. |
| D4 | No new architecture — reuse ALRT-1 seams | `Notifier` trait + `keyring`/process abstractions already exist; this is conditional compilation, not redesign. No ARD required. |
| D5 | **One platform façade owns all `cfg(target_os)`** | SOLID/DRY: a single `platform` module is the only place that branches on OS; call sites stay platform-agnostic. See §3a. |

## 3a. SOLID + DRY design rules (binding for this ticket)

The porting must not sprinkle `cfg(target_os = …)` across `main.rs`, `notifier.rs`, etc. There is
currently **zero** `cfg` in the codebase — we keep it that way at the call sites by funnelling every
OS branch through one seam.

**New module `crates/app/src/platform.rs` — the single platform façade (SRP):**
- Exposes a **compile-time-selected** notifier via a type alias, so static dispatch and the existing
  generic `NotificationDispatcher<N>` are preserved (no `Box<dyn>`, no runtime cost), plus the one
  OS-divergent command:
  ```rust
  pub type PlatformNotifier = crate::desktop_notifier::DesktopNotifier; // cross-platform today
  pub fn notifier() -> PlatformNotifier { PlatformNotifier::default() }
  pub fn open_url(url: &str) { /* the ONLY `open` vs xdg-open branch */ }
  ```
- The notification backend is cross-platform (one `DesktopNotifier` over `notify-rust`; see §4.3), so
  the alias needs no `#[cfg]` today. When a platform needs a bespoke backend, add one `#[cfg]` arm to
  the alias here — **no edits to call sites** (OCP).

**How each SOLID principle is honoured**
- **SRP** — one module per OS backend; the façade's sole job is OS selection.
- **OCP** — new platform = new impl behind the existing `Notifier` trait + a `cfg` arm in the façade;
  `NotificationDispatcher`, `main.rs`, and the update loop are untouched.
- **LSP** — every backend satisfies the same `Notifier` contract (`notify(summary, body)`), so they
  are interchangeable by construction.
- **DIP** — `main.rs` depends on the `Notifier` trait + the `platform` façade, never on `XdgNotifier`
  / `MacNotifier` / `xdg-open` directly.

**DRY checkpoints**
- Exactly **one** `cfg`-branch per concern (notifier selection; browser-open command).
- Keychain backend is selected **once** via target-conditional Cargo features (§4.1) — uniform
  `keyring::Entry` call sites, no per-OS code.
- CI build+test is a **single matrixed job** over `[ubuntu, macos]`, not two copy-pasted jobs (§5).
- Reuse the existing `Notifier` test fake for both backends — no per-OS test scaffolding.

## 4. Platform-seam work

### 4.1 Keychain — `crates/store`
`crates/store/Cargo.toml:20` pins keyring to the Linux Secret-Service backend
(`async-secret-service`, `async-io`, `crypto-rust`) **unconditionally**. On macOS that backend is
wrong; macOS uses the Security-framework backend (`apple-native`).

- Replace the single `keyring` dep with **target-conditional** dependency tables:
  - `cfg(target_os = "linux")` → `["async-secret-service", "async-io", "crypto-rust"]` (unchanged)
  - `cfg(target_os = "macos")` → `["apple-native"]`
- Verify `crates/store/src/keychain.rs` (the synchronous `Entry` API wrapper) needs no change —
  the `Entry` API is uniform across backends; the Linux comment about "blocked-on async backend"
  is internal to the feature and not referenced in our code.

### 4.2 Open-in-browser — `crates/app/src/main.rs:454` (`open_url`)
Hard-codes `xdg-open`. **Move `open_url` into `platform::open_url`** (§3a) — the function leaves
`main.rs` entirely and becomes the single `cfg`-branch for the launch command:
- `cfg(target_os = "macos")` → `open`
- else → `xdg-open`

`main.rs` call sites change from `open_url(url)` to `platform::open_url(url)` — no `cfg` at the call
site (DIP).

### 4.3 Notifications — `crates/app`
`xdg_notifier.rs` (over `notify-rust`) was the only `Notifier` impl; `main.rs:78,185` hard-coded it
as the concrete type in `NotificationDispatcher<XdgNotifier>`.

**DRY decision (revised during implementation):** the impl only uses `notify-rust`'s portable
`summary`/`body`/`show` surface, and `notify-rust` is itself cross-platform (freedesktop D-Bus on
Linux, `NSUserNotification`/`mac-notification-sys` on macOS). A separate `MacNotifier` would be a
byte-for-byte copy — a DRY violation. So instead of two backends:
- Rename `xdg_notifier.rs` → `desktop_notifier.rs`, `XdgNotifier` → `DesktopNotifier` (the "XDG" name
  was Linux-specific and misleading); one cross-platform backend serves every target.
- `platform::PlatformNotifier` aliases it and `platform::notifier()` constructs it; `main.rs` stops
  naming any concrete backend. If a platform ever needs a bespoke backend, add a `#[cfg]` arm to the
  alias in `platform.rs` — call sites and the dispatcher never change (OCP).
- **Known limitation:** macOS only displays user notifications for a properly bundled, identified
  app. An unsigned, non-`.app` dev binary may suppress banners. The seam must compile and
  best-effort fire; full notification delivery is gated on the deferred `.app`-bundle/signing
  ticket. This caveat goes in the docs and is **not** an acceptance blocker for this ticket.

### 4.4 Config/data paths
No code change expected — the `directories` crate already resolves macOS paths
(`~/Library/Application Support`). Verify the SQLite cache + config land there on macOS.

## 5. CI — `.github/workflows/ci.yml`
**DRY:** extend the existing job into a `matrix.os: [ubuntu-latest, macos-14]` rather than copy-paste
a second job — shared steps (checkout, toolchain, `cargo test --workspace`) stay single-sourced;
only the universal-binary build is gated behind `if: runner.os == 'macOS'`.
- Runner: `macos-14` (Apple Silicon). Add the `x86_64-apple-darwin` target via `rustup`.
- Build both targets; `lipo -create -output alurtmee` to produce the universal binary; verify with
  `lipo -info` / `file`.
- Run `cargo test --workspace` (native arch).
- Upload the universal binary as a CI artifact (no release attachment — `release.yml` untouched).

## 6. Acceptance criteria
1. `cargo build --workspace` and `cargo test --workspace` pass on macOS in CI.
2. CI produces a universal binary; `lipo -info` reports both `x86_64` and `arm64`.
3. Launching the binary on macOS opens the Iced window and the app runs (auth → poll → list).
4. PAT round-trips through the macOS Keychain (store/read/delete) — verified by the existing
   keychain tests on the macOS runner.
5. "Open PR on GitHub" launches the default browser via `open`.
6. Linux build/test/release remain green and unchanged (no regression).
7. README/docs document macOS dev-build steps and the unsigned-Gatekeeper caveat.
8. **SOLID/DRY:** all `cfg(target_os)` lives in `crates/app/src/platform.rs` (+ the keychain Cargo
   target tables); `grep -rn 'cfg(target_os' crates/app/src` outside `platform.rs` returns nothing,
   and `main.rs` names no concrete notifier/opener backend.

## 7. Risks / open questions
- **R1 — keyring `apple-native` feature name/behavior on v3.x.** Confirm exact feature flag and
  that `Entry` works synchronously on macOS. *Mitigation:* validated by CI keychain tests early.
- **R2 — notifications on unsigned dev binary** (see §4.3). Accepted limitation, documented.
- **R3 — iced/wgpu→Metal** on the CI runner (headless). *Mitigation:* GUI smoke is manual on a real
  Mac; CI covers build+test+keychain only. CI does not assert a window renders.
- **R4 — `cargo test` needing a real keychain in CI.** macOS runners have a login keychain;
  confirm tests can create/unlock a keychain entry non-interactively, else gate behind a feature.

## 8. Task breakdown (suggested PRs)
This is small enough for **one PR**, but can split if CI iteration is noisy:
1. **Platform façade** — add `crates/app/src/platform.rs`, move `open_url` in, rename the notifier to
   the cross-platform `DesktopNotifier`, introduce the `PlatformNotifier` alias, and repoint
   `main.rs` (§3a, §4.2–4.3).
2. Keychain target-conditional Cargo features (§4.1) + path verification (§4.4).
3. macOS CI matrix + universal-binary build (§5).
4. Docs (§6.7).

## 9. Follow-up tickets (out of scope here)
- **ALRT-26+ (suggested):** macOS `.app` bundle + `.dmg`, code signing + notarization, macOS
  release pipeline. Requires an Apple Developer account ($99/yr).
- Universal-binary **release** artifacts (this ticket only emits a CI artifact).
