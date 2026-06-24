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
Hard-codes `xdg-open`. Make the command `cfg`-selected:
- `cfg(target_os = "macos")` → `open`
- else → `xdg-open`

### 4.3 Notifications — `crates/app`
`xdg_notifier.rs` (over `notify-rust`) is the only `Notifier` impl, selected in `main.rs`. Add a
macOS path:
- Wire a macOS `Notifier` (via `notify-rust`, which has a macOS backend) and select the impl by
  `cfg(target_os)` where `XdgNotifier` is constructed.
- **Known limitation:** macOS only displays user notifications for a properly bundled, identified
  app. An unsigned, non-`.app` dev binary may suppress banners. The seam must compile and
  best-effort fire; full notification delivery is gated on the deferred `.app`-bundle/signing
  ticket. This caveat goes in the docs and is **not** an acceptance blocker for this ticket.

### 4.4 Config/data paths
No code change expected — the `directories` crate already resolves macOS paths
(`~/Library/Application Support`). Verify the SQLite cache + config land there on macOS.

## 5. CI — `.github/workflows/ci.yml`
Add a macOS job alongside the Linux one:
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
1. Seam ports (§4.1–4.3) + path verification (§4.4).
2. macOS CI job + universal-binary build (§5).
3. Docs (§6.7).

## 9. Follow-up tickets (out of scope here)
- **ALRT-26+ (suggested):** macOS `.app` bundle + `.dmg`, code signing + notarization, macOS
  release pipeline. Requires an Apple Developer account ($99/yr).
- Universal-binary **release** artifacts (this ticket only emits a CI artifact).
