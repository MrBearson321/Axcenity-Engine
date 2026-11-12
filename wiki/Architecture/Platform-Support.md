---
title: Platform support
type: architecture
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [Cargo.toml, crates/assets/src/lib.rs, crates/save/src/lib.rs, scripts/check-platform.sh, scripts/check-platform.ps1]
docs: [docs/architecture/portability.md, docs/development-policy.md, docs/rendering/pipeline.md, docs/project/releases.md]
tags: [architecture, platforms]
---

# Platform support

| Platform | Status | Verified on changes? |
|---|---|---|
| Windows (x86-64) | **Primary** | Yes: `scripts\check-platform.ps1` |
| Linux (x86-64) | **Primary** | Yes: `scripts/check-platform.sh`, including the server with no display |
| macOS | Secondary: builds and ships in releases | Not required |
| Android, iOS | **Deferred** (Milestone 20, Phases 82–88) | — |
| Web | **Deferred**; Luau native code has no wasm path | — |

Decision and revisit point: [ADR-0011](Architecture-Decisions/ADR-0011-Desktop-First-Local-Verification.md) and `docs/development-policy.md#platforms`.

## Where platform differences live

There is no platform abstraction layer of the engine's own. Each concern goes through one library or one module:

| Concern | Through | Per-OS detail |
|---|---|---|
| Window and input | `winit` 0.30 (player), eframe (editor) | Wayland and X11 both on in eframe |
| GPU | `wgpu` 30 | DirectX 12 or Vulkan on Windows, Vulkan on Linux, Metal on macOS. "Avoid features that aren't available on all three" (`docs/rendering/pipeline.md`); optional features are read as `Capabilities` and lowered with a note ([ADR-0013](Architecture-Decisions/ADR-0013-Look-Versus-Cost-Settings.md)) |
| Texture compression | BC7/BC5 (`TEXTURE_COMPRESSION_BC`) | Every desktop GPU and lavapipe; falls back to RGBA8 where missing. ASTC/ETC2 not needed while mobile is deferred |
| Game controllers | `gilrs` 0.11 | Windows.Gaming.Input, IOKit, evdev via libudev (`libudev-dev` to build on Linux) |
| File watching | `notify` 8 | inotify, FSEvents, ReadDirectoryChangesW, with a polling fallback |
| Dialogs | `rfd` 0.17 | Desktop portal on Linux |
| Editor menus | egui in the header; `muda` 0.21 on macOS | macOS uses the system menu bar (`app/native_menu.rs`, `docs/editor/shell.md`); Windows and Linux draw the menus in the window's own title bar |
| Audio | `kira` 0.12 over `cpal` | ALSA on Linux |
| Game file access | `assets` only (`safe_path`) | Never `std::fs` in gameplay code, so other platforms stay possible |
| Save files | `save::game_folder` | The player's data folder per OS; the one exception to "files through `assets`" |
| Script native code | `mlua` `luau-jit` | x86-64 and AArch64; elsewhere scripts stay interpreted |
| Texture cooking | `intel_tex_2` prebuilt ISPC kernels | Windows, Linux, macOS on x86-64 and arm64; editor only |

## Known platform issues

- **AMD, Vulkan, Windows: the sky clear isn't sRGB-encoded** through an sRGB view while shaded pixels are, so the sky can differ by machine (`docs/rendering/pipeline.md`, "Known issue"). See [Known limitations](../Roadmap/Known-Limitations.md).
- GPU tests share one GPU; nextest runs at most 8 of them at once or wgpu times out (`.config/nextest.toml`).
- Machine-specific observations (VRR flicker on an OLED display, CCD pinning on the 9950X3D) are in [Hardware notes](../Performance/Hardware-Notes.md).

## Checking a platform

`scripts/check-platform.sh` / `check-platform.ps1` run build, the "player has no editor code" check, clippy, every test, doc tests, `axcenity-test examples/hello-character` and the dedicated server with no display, and save a summary to `target/platform-check-<os>.txt`. Packages and Build Game per OS: `python3 scripts/check-packages.py`. See [Build and run](../Getting-Started/Build-and-Run.md).
