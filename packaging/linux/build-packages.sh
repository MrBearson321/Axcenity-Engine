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
  for file in axcenity-editor axcenity-player tools/luau-lsp players/macos/axcenity-player players/linux/axcenity-player; do
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

GENERATED="$ROOT/packaging/linux/.generated"
mkdir -p "$OUT" "$GENERATED"
export NFPM_VERSION NFPM_PRERELEASE_LINE STAGE ROOT

# install.json for one kind of package (see packaging/stage.py).
install_marker() {
  printf '{"schema": 1, "kind": "%s"}\n' "$1"
}

# The nfpm config for one package format, with its own install.json.
# envsubst replaces ${VAR}; the template has no other dollars.
nfpm_config() {
  local kind="$1"
  INSTALL_JSON="$GENERATED/install-$kind.json"
  install_marker "$kind" > "$INSTALL_JSON"
  export INSTALL_JSON
  envsubst '${NFPM_VERSION} ${NFPM_PRERELEASE_LINE} ${STAGE} ${ROOT} ${INSTALL_JSON}' \
    < "$ROOT/packaging/linux/nfpm.yaml.tpl" \
    > "$GENERATED/nfpm-$kind.yaml"
  echo "$GENERATED/nfpm-$kind.yaml"
}

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

if wants deb; then
  nfpm package --config "$(nfpm_config deb)" --packager deb --target "$OUT/${FILE_STEM}.deb"
fi
if wants rpm; then
  nfpm package --config "$(nfpm_config rpm)" --packager rpm --target "$OUT/${FILE_STEM}.rpm"
fi
if wants archlinux; then
  nfpm package --config "$(nfpm_config pacman)" --packager archlinux --target "$OUT/${FILE_STEM}.pkg.tar.zst"
fi

# Portable tree: extract and run ./axcenity-editor. It is also the update
# payload the editor installs, so it keeps the stage's install.json.
if wants tar; then
  TAR_DIR="$OUT/${FILE_STEM}"
  rm -rf "$TAR_DIR"
  mkdir -p "$TAR_DIR"
  cp -a "$STAGE/." "$TAR_DIR/"
  install_marker linux-tarball > "$TAR_DIR/install.json"
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
install_marker appimage > "$APPDIR/usr/bin/install.json"
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
Exec=AppRun %U
Icon=axcenity-editor
Terminal=false
Categories=Development;IDE;Graphics;
StartupNotify=true
MimeType=x-scheme-handler/axcenity;
EOF
cp "$APPDIR/axcenity-editor.desktop" "$APPDIR/usr/share/applications/axcenity-editor.desktop"
cp "$STAGE/LICENSE" "$STAGE/NOTICE" "$STAGE/INSTALL.txt" "$APPDIR/usr/share/doc/axcenity-editor/"

export APPIMAGE_EXTRACT_AND_RUN=1
ARCH=x86_64 appimagetool "$APPDIR" "$OUT/${FILE_STEM}.AppImage"
chmod +x "$OUT/${FILE_STEM}.AppImage"
rm -rf "$APPDIR"

echo "linux packages in $OUT"
