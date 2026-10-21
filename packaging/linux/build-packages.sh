#!/usr/bin/env bash
# Builds .deb, .rpm, .pkg.tar.zst, .tar.gz and AppImage from a staged payload.
# Usage: build-packages.sh <version> <repo-root> <staged-payload> <out-dir>
set -euo pipefail

VERSION="${1:?version}"
ROOT="${2:?repo root}"
STAGE="${3:?staged payload}"
OUT="${4:?output directory}"

if [[ "$VERSION" == *-* ]]; then
  NFPM_VERSION="${VERSION%%-*}"
  PRE="${VERSION#*-}"
  NFPM_PRERELEASE_LINE="prerelease: ${PRE}"$'\n'
else
  NFPM_VERSION="$VERSION"
  NFPM_PRERELEASE_LINE=""
fi

mkdir -p "$OUT" "$ROOT/packaging/linux/.generated"
export NFPM_VERSION NFPM_PRERELEASE_LINE STAGE ROOT
# envsubst replaces ${VAR}; the template has no other dollars.
envsubst '${NFPM_VERSION} ${NFPM_PRERELEASE_LINE} ${STAGE} ${ROOT}' \
  < "$ROOT/packaging/linux/nfpm.yaml.tpl" \
  > "$ROOT/packaging/linux/.generated/nfpm.yaml"

python3 - <<'PY'
from pathlib import Path
from PIL import Image
import os
root = Path(os.environ["ROOT"])
out = root / "packaging" / "linux" / ".generated"
src = Image.open(root / "assets" / "branding" / "icon.png").convert("RGBA")
out.mkdir(parents=True, exist_ok=True)
for size in (32, 48, 64, 128, 256, 512):
    src.resize((size, size), Image.LANCZOS).save(out / f"axcenity-editor-{size}.png", optimize=True)
PY

FILE_STEM="AxcenityEditor-${VERSION}-linux-x86_64"

nfpm package --config "$ROOT/packaging/linux/.generated/nfpm.yaml" \
  --packager deb --target "$OUT/${FILE_STEM}.deb"
nfpm package --config "$ROOT/packaging/linux/.generated/nfpm.yaml" \
  --packager rpm --target "$OUT/${FILE_STEM}.rpm"
nfpm package --config "$ROOT/packaging/linux/.generated/nfpm.yaml" \
  --packager archlinux --target "$OUT/${FILE_STEM}.pkg.tar.zst"

# Portable tree: extract and run ./axcenity-editor.
TAR_DIR="$OUT/${FILE_STEM}"
rm -rf "$TAR_DIR"
mkdir -p "$TAR_DIR"
cp -a "$STAGE/." "$TAR_DIR/"
chmod +x "$TAR_DIR/axcenity-editor" "$TAR_DIR/axcenity-player" \
  "$TAR_DIR/players/macos/axcenity-player" "$TAR_DIR/players/linux/axcenity-player"
tar -C "$OUT" -czf "$OUT/${FILE_STEM}.tar.gz" "$(basename "$TAR_DIR")"
rm -rf "$TAR_DIR"

# AppImage: editor and players beside each other under usr/bin.
APPDIR="$OUT/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$APPDIR/usr/share/doc/axcenity-editor"
cp -a "$STAGE/." "$APPDIR/usr/bin/"
chmod +x "$APPDIR/usr/bin/axcenity-editor" "$APPDIR/usr/bin/axcenity-player" \
  "$APPDIR/usr/bin/players/macos/axcenity-player" "$APPDIR/usr/bin/players/linux/axcenity-player"
cp "$ROOT/packaging/linux/AppRun" "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun"
cp "$ROOT/packaging/linux/.generated/axcenity-editor-256.png" "$APPDIR/axcenity-editor.png"
cp "$ROOT/packaging/linux/.generated/axcenity-editor-256.png" \
  "$APPDIR/usr/share/icons/hicolor/256x256/apps/axcenity-editor.png"
cat > "$APPDIR/axcenity-editor.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Axcenity Editor
Comment=Create 3D games with parts, Lua and blocks
Exec=AppRun
Icon=axcenity-editor
Terminal=false
Categories=Development;IDE;Graphics;
StartupNotify=true
EOF
cp "$APPDIR/axcenity-editor.desktop" "$APPDIR/usr/share/applications/axcenity-editor.desktop"
cp "$STAGE/LICENSE" "$STAGE/NOTICE" "$STAGE/INSTALL.txt" "$APPDIR/usr/share/doc/axcenity-editor/"

export APPIMAGE_EXTRACT_AND_RUN=1
ARCH=x86_64 appimagetool "$APPDIR" "$OUT/${FILE_STEM}.AppImage"
chmod +x "$OUT/${FILE_STEM}.AppImage"
rm -rf "$APPDIR"

echo "linux packages in $OUT"
