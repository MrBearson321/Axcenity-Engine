#!/usr/bin/env bash
# Builds Axcenity Editor.app and a branded drag-to-Applications .dmg.
# Usage: build-dmg.sh <version> <repo-root> <staged-payload> <outfile.dmg>
set -euo pipefail

VERSION="${1:?version}"
ROOT="${2:?repo root}"
STAGE="${3:?staged payload}"
OUTFILE="${4:?output dmg}"

SHORT_VERSION="${VERSION%%-*}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/axcenity-dmg.XXXXXX")"
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

APP="$WORK/Axcenity Editor.app"
MACOS="$APP/Contents/MacOS"
RES="$APP/Contents/Resources"

mkdir -p "$MACOS" "$RES" "$WORK/icon.iconset"

sips -z 16 16     "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_16x16.png" >/dev/null
sips -z 32 32     "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_16x16@2x.png" >/dev/null
sips -z 32 32     "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_32x32.png" >/dev/null
sips -z 64 64     "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_32x32@2x.png" >/dev/null
sips -z 128 128   "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_128x128.png" >/dev/null
sips -z 256 256   "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_128x128@2x.png" >/dev/null
sips -z 256 256   "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_256x256.png" >/dev/null
sips -z 512 512   "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_256x256@2x.png" >/dev/null
sips -z 512 512   "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_512x512.png" >/dev/null
sips -z 1024 1024 "$ROOT/assets/branding/icon.png" --out "$WORK/icon.iconset/icon_512x512@2x.png" >/dev/null
iconutil -c icns "$WORK/icon.iconset" -o "$RES/AppIcon.icns"

sed -e "s/__VERSION__/${VERSION}/g" -e "s/__SHORT_VERSION__/${SHORT_VERSION}/g" \
  "$ROOT/packaging/macos/Info.plist" > "$APP/Contents/Info.plist"

cp -R "$STAGE/." "$MACOS/"
cp "$STAGE/LICENSE" "$STAGE/NOTICE" "$STAGE/INSTALL.txt" "$RES/"
chmod +x "$MACOS/axcenity-editor" "$MACOS/axcenity-player" \
  "$MACOS/players/macos/axcenity-player" "$MACOS/players/linux/axcenity-player"

IDENTITY="-"
if security find-identity -v -p codesigning 2>/dev/null | grep -q "Developer ID Application"; then
  IDENTITY="$(security find-identity -v -p codesigning | awk -F'\"' '/Developer ID Application/ { print $2; exit }')"
fi
codesign --force --deep --sign "$IDENTITY" "$APP"

mkdir -p "$(dirname "$OUTFILE")"
rm -f "$OUTFILE"

export AXCENITY_APP="$APP"
export AXCENITY_ICNS="$RES/AppIcon.icns"
export AXCENITY_DMG_BG="$ROOT/packaging/assets/dmg-background.png"
export AXCENITY_LICENSE="$ROOT/LICENSE"

python3 -m venv "$WORK/venv"
"$WORK/venv/bin/pip" install --quiet dmgbuild
"$WORK/venv/bin/dmgbuild" -s "$ROOT/packaging/macos/dmgbuild_settings.py" "Axcenity Editor" "$OUTFILE"

echo "wrote $OUTFILE"
