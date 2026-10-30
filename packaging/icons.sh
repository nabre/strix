#!/usr/bin/env bash
# Modified by the Strix contributors (fork of WordCraft); see NOTICE.
# Regenerate every app icon from assets/app-icon/strix.svg, and the wordmark images the app
# draws (assets/brand/) from docs/brand/wordmark-{light,dark}.svg.
#
# Needs: resvg (brew install resvg / cargo install resvg). Runs on any OS: the .ico and .icns are
# packed by `cargo xtask ico|icns`. The outputs are committed, so builds and packaging never need
# these tools.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
SVG="$DIR/strix.svg"
ID="io.github.nabre.strix"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v resvg >/dev/null || { echo "error: resvg not found (brew install resvg)" >&2; exit 1; }

# The artwork is a full-bleed 512 tile (rx=112). Windows and Linux use it as is. macOS icons sit on
# Apple's grid: an 824/1024 body with a transparent margin, so widen the viewBox to 512*1024/824.
MAC="$TMP/mac.svg"
sed 's/viewBox="0 0 512 512"/viewBox="-62.136 -62.136 636.272 636.272"/' "$SVG" >"$MAC"
grep -q 'viewBox="-62.136' "$MAC" || { echo "error: expected viewBox=\"0 0 512 512\" in $SVG" >&2; exit 1; }

render() {
  local src="$1" size="$2" dst="$3"
  resvg -w "$size" -h "$size" "$src" "$dst" </dev/null
  return 0
}

render "$SVG" 1024 "$DIR/strix-1024.png"
# Runtime window/Dock icon on macOS (embedded by apps/strix); other platforms use hicolor 256.
render "$MAC" 512 "$DIR/strix-macos-512.png"

# Linux hicolor theme.
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  render "$SVG" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
mkdir -p "$DIR/hicolor/scalable/apps"
cp "$SVG" "$DIR/hicolor/scalable/apps/$ID.svg"

# Windows .ico.
ICO_PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  render "$SVG" "$s" "$TMP/ico-$s.png"
  ICO_PNGS+=("$TMP/ico-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- ico "$DIR/strix.ico" "${ICO_PNGS[@]}")

# macOS .icns: every iconset size, on Apple's grid.
ICNS_PNGS=()
for s in 16 32 64 128 256 512 1024; do
  render "$MAC" "$s" "$TMP/icns-$s.png"
  ICNS_PNGS+=("$TMP/icns-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- icns "$DIR/strix.icns" "${ICNS_PNGS[@]}")

# The wordmark as the app draws it (About, the empty window, the File page): 192 px tall, enough
# for its largest use at 2x.
BRAND="$ROOT/assets/brand"
mkdir -p "$BRAND"
for v in light dark; do
  resvg -h 192 "$ROOT/docs/brand/wordmark-$v.svg" "$BRAND/wordmark-$v.png" </dev/null
done
echo "icons written to $DIR and $BRAND"
