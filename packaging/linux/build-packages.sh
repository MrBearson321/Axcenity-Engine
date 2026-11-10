#!/usr/bin/env bash
# Builds .deb, .rpm, .pkg.tar.zst, .tar.gz and AppImage from a staged payload.
# Usage: build-packages.sh <version> <repo-root> <staged-payload> <out-dir> [formats]
# formats: any of "deb rpm archlinux tar appimage" (default: all). deb, rpm
# and archlinux need nfpm; appimage needs appimagetool. A local check
# (scripts/check-packages.py) asks only for the ones whose tools it finds.
set -euo pipefail

VERSION="${1:?version}"
ROOT="${2:?repo root}"
STAGE="${3:?staged payload}"
OUT="${4:?output directory}"
FORMATS=" ${5:-deb rpm archlinux tar appimage} "
wants() { [[ "$FORMATS" == *" $1 "* ]]; }

# The executables in a payload (other-OS players a local stage left out
# aren't there). The Windows player needs no mark.
mark_executables() {
  local dir="$1" file
  for file in axcenity-editor axcenity-player players/macos/axcenity-player players/linux/axcenity-player; do
    if [ -e "$dir/$file" ]; then chmod +x "$dir/$file"; fi
  done
}

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

wants deb && nfpm package --config "$ROOT/packaging/linux/.generated/nfpm.yaml" \
  --packager deb --target "$OUT/${FILE_STEM}.deb"
wants rpm && nfpm package --config "$ROOT/packaging/linux/.generated/nfpm.yaml" \
  --packager rpm --target "$OUT/${FILE_STEM}.rpm"
wants archlinux && nfpm package --config "$ROOT/packaging/linux/.generated/nfpm.yaml" \
  --packager archlinux --target "$OUT/${FILE_STEM}.pkg.tar.zst"

# Portable tree: extract and run ./axcenity-editor.
if wants tar; then
  TAR_DIR="$OUT/${FILE_STEM}"
  rm -rf "$TAR_DIR"
  mkdir -p "$TAR_DIR"
  cp -a "$STAGE/." "$TAR_DIR/"
  mark_executables "$TAR_DIR"
  tar -C "$OUT" -czf "$OUT/${FILE_STEM}.tar.gz" "$(basename "$TAR_DIR")"
  rm -rf "$TAR_DIR"
fi

if ! wants appimage; then
  echo "linux packages in $OUT"
  exit 0
fi

# AppImage: editor and players beside each other under usr/bin.
APPDIR="$OUT/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$APPDIR/usr/share/doc/axcenity-editor"
cp -a "$STAGE/." "$APPDIR/usr/bin/"
mark_executables "$APPDIR/usr/bin"
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
