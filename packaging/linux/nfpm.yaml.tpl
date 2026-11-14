# Filled in by packaging/linux/build-packages.sh.
name: axcenity-editor
arch: amd64
platform: linux
version: ${NFPM_VERSION}
${NFPM_PRERELEASE_LINE}release: "1"
section: devel
priority: optional
maintainer: Paul Janota and contributors <MrBearson321@users.noreply.github.com>
description: |-
  The Axcenity Engine editor.
  Build worlds from parts, script them in Lua 5.4 or with blocks, press Play
  to test, and export standalone games for Windows, macOS and Linux.
vendor: Axcenity Engine
homepage: https://github.com/MrBearson321/Axcenity-Engine
license: Apache-2.0
rpm:
  packager: Paul Janota and contributors
archlinux:
  packager: Paul Janota and contributors
contents:
  - src: ${STAGE}/axcenity-editor
    dst: /opt/axcenity/axcenity-editor
    file_info:
      mode: 0755
  - src: ${STAGE}/axcenity-player
    dst: /opt/axcenity/axcenity-player
    file_info:
      mode: 0755
  - src: ${STAGE}/players
    dst: /opt/axcenity/players
  - src: ${STAGE}/tools/luau-lsp
    dst: /opt/axcenity/tools/luau-lsp
    file_info:
      mode: 0755
  - src: ${STAGE}/tools/luau-lsp-LICENSE.txt
    dst: /opt/axcenity/tools/luau-lsp-LICENSE.txt
  - src: ${STAGE}/LICENSE
    dst: /opt/axcenity/LICENSE
  - src: ${STAGE}/NOTICE
    dst: /opt/axcenity/NOTICE
  - src: ${STAGE}/INSTALL.txt
    dst: /opt/axcenity/INSTALL.txt
  - src: ${INSTALL_JSON}
    dst: /opt/axcenity/install.json
  - src: /opt/axcenity/axcenity-editor
    dst: /usr/bin/axcenity-editor
    type: symlink
  - src: /opt/axcenity/axcenity-player
    dst: /usr/bin/axcenity-player
    type: symlink
  - src: ${ROOT}/packaging/linux/axcenity-editor.desktop
    dst: /usr/share/applications/axcenity-editor.desktop
  - src: ${ROOT}/packaging/linux/.generated/axcenity-editor-32.png
    dst: /usr/share/icons/hicolor/32x32/apps/axcenity-editor.png
  - src: ${ROOT}/packaging/linux/.generated/axcenity-editor-48.png
    dst: /usr/share/icons/hicolor/48x48/apps/axcenity-editor.png
  - src: ${ROOT}/packaging/linux/.generated/axcenity-editor-64.png
    dst: /usr/share/icons/hicolor/64x64/apps/axcenity-editor.png
  - src: ${ROOT}/packaging/linux/.generated/axcenity-editor-128.png
    dst: /usr/share/icons/hicolor/128x128/apps/axcenity-editor.png
  - src: ${ROOT}/packaging/linux/.generated/axcenity-editor-256.png
    dst: /usr/share/icons/hicolor/256x256/apps/axcenity-editor.png
  - src: ${ROOT}/packaging/linux/.generated/axcenity-editor-512.png
    dst: /usr/share/icons/hicolor/512x512/apps/axcenity-editor.png
  - src: ${STAGE}/LICENSE
    dst: /usr/share/doc/axcenity-editor/LICENSE
  - src: ${STAGE}/NOTICE
    dst: /usr/share/doc/axcenity-editor/NOTICE
  - src: ${STAGE}/INSTALL.txt
    dst: /usr/share/doc/axcenity-editor/INSTALL.txt
overrides:
  deb:
    depends:
      - libc6
      - libasound2 | libasound2t64
      - libdbus-1-3
      - libxkbcommon0
    recommends:
      - mesa-vulkan-drivers
      - libwayland-client0
      - libx11-6
      # winit loads it at start on X11; without it the window can't open.
      - libxkbcommon-x11-0
  rpm:
    depends:
      - alsa-lib
      - dbus-libs
      - libxkbcommon
    recommends:
      - mesa-vulkan-drivers
      - libX11
      - libxkbcommon-x11
      - libwayland-client
  archlinux:
    depends:
      - alsa-lib
      - dbus
      - libxkbcommon
      - libxkbcommon-x11
      - vulkan-icd-loader
