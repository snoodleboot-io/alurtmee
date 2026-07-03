#!/usr/bin/env bash
#
# Assemble an unsigned universal Alurtmee.app and package it as a .dmg (ALRT-26).
#
# Usage: packaging/macos/make-dmg.sh <version> <universal-binary> <out-dir>
#   <version>           e.g. 0.1.0 — substituted into Info.plist and the file/volume name
#   <universal-binary>  a lipo'd x86_64+arm64 `alurtmee` binary
#   <out-dir>           directory the .dmg is written to
#
# macOS-only: uses sips / iconutil / hdiutil. Runs in the release workflow's macos job; there is no
# Linux equivalent, so it is exercised for the first time in CI (guard locally with shellcheck).
set -euo pipefail

VERSION="${1:?version required}"
BIN="${2:?universal binary path required}"
OUT_DIR="${3:?output dir required}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

APP="$WORK/Alurtmee.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# Info.plist with the version substituted in.
sed "s/__VERSION__/${VERSION}/g" "$REPO_ROOT/packaging/macos/Info.plist" \
  > "$APP/Contents/Info.plist"

# The universal binary.
cp "$BIN" "$APP/Contents/MacOS/alurtmee"
chmod +x "$APP/Contents/MacOS/alurtmee"

# AppIcon.icns from the high-res logo. iconutil needs the standard-named iconset entries.
ICONSET="$WORK/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$REPO_ROOT/assets/logo.png" \
    --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  retina=$((size * 2))
  sips -z "$retina" "$retina" "$REPO_ROOT/assets/logo.png" \
    --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

# Stage the .app beside an /Applications symlink so the .dmg offers drag-to-install.
STAGE="$WORK/stage"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

mkdir -p "$OUT_DIR"
DMG="$OUT_DIR/Alurtmee-${VERSION}-universal.dmg"
rm -f "$DMG"
hdiutil create \
  -volname "Alurtmee ${VERSION}" \
  -srcfolder "$STAGE" \
  -ov -format UDZO \
  "$DMG"

echo "built: $DMG"
lipo -info "$APP/Contents/MacOS/alurtmee"
