#!/usr/bin/env bash
# ATDD acceptance smoke (Phase 0): "running the app opens a window and renders without panicking."
#
# Builds and launches the `alurtmee` binary, lets it run long enough to create its window and
# paint at least one frame, then asserts the process is still alive (i.e. it did not panic on
# startup) and terminates it cleanly. Surviving the dwell = PASS.
#
# Works headless: if no DISPLAY is set, it self-wraps in `xvfb-run`. Software rendering is forced
# (LIBGL_ALWAYS_SOFTWARE=1) so it passes on machines/CI runners without a GPU.
set -euo pipefail

DWELL_SECONDS="${DWELL_SECONDS:-4}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Re-exec under a virtual X server when there is no display available.
if [[ -z "${DISPLAY:-}" && "${SMOKE_XVFB_WRAPPED:-}" != "1" ]]; then
  export SMOKE_XVFB_WRAPPED=1
  exec xvfb-run -a --server-args="-screen 0 1280x800x24" "${BASH_SOURCE[0]}" "$@"
fi

export LIBGL_ALWAYS_SOFTWARE="${LIBGL_ALWAYS_SOFTWARE:-1}"

cargo build -p app --bin alurtmee
BIN="$REPO_ROOT/target/debug/alurtmee"

"$BIN" &
APP_PID=$!

sleep "$DWELL_SECONDS"

if kill -0 "$APP_PID" 2>/dev/null; then
  echo "window smoke: app alive after ${DWELL_SECONDS}s — PASS"
  kill "$APP_PID" 2>/dev/null || true
  wait "$APP_PID" 2>/dev/null || true
  exit 0
else
  # Process already exited — capture its status for the failure message.
  wait "$APP_PID" && status=0 || status=$?
  echo "window smoke: app exited early (status ${status}) — FAIL"
  exit 1
fi
