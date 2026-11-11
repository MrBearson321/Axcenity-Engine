---
title: Dependencies
type: reference
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [Cargo.toml, .config/hakari.toml]
docs: [docs/architecture/crates.md, docs/development.md]
tags: [reference, dependencies]
---

# Dependencies

Third-party crates pinned in the root `Cargo.toml` `[workspace.dependencies]` (versions as of `21aadbf`). The comment beside each pin in `Cargo.toml` says why it's there and its license; this page groups them. Engine-internal edges are [System dependencies](../Architecture/System-Dependencies.md).

| Area | Crate (pin) | Used by | License | Notes |
|---|---|---|---|---|
| GPU | `wgpu` 30 (M12, PR #104) | render, editor (via eframe) | MIT/Apache | Moves with winit and eframe |
| Window | `winit` 0.30 | player, editor | Apache | Left out of the workspace hack so the server builds no window crates |
| Editor shell | `eframe` 0.36 (wgpu backend, persistence, accesskit, wayland, x11), `egui_dock` 0.21, `egui-phosphor` 0.14, `rfd` 0.17 | editor | MIT/Apache | Upgrade together with wgpu |
| Text | `glyphon` 0.12 | render | MIT/Apache/Zlib | cosmic-text shaping |
| Math | `glam` 0.34 | everywhere | MIT/Apache | rapier uses `glamx`; several glam versions in the lock |
| Physics | `rapier3d` 0.36 (`parallel`), **patched** from `vendor/rapier3d` (`[patch.crates-io]`, PR #121) so pairs with a contact hook (Ice, Rubber) keep their contacts between steps; see `vendor/README.md` | physics | Apache | parry3d for shapes and decompositions |
| Scripting | `mlua` 0.12 (`luau`, `luau-jit`), `luau0-src` **=0.22.0** | script | MIT | Pin moves with mlua; guarded by `luau_pin_tests.rs` |
| Audio | `kira` 0.12, `mint` 0.5 | audio | MIT/Apache | cpal underneath (ALSA on Linux) |
| Models | `gltf` 1.4 (`KHR_texture_transform`, `extras`), `ufbx` 0.11, `meshopt` 0.7 | assets | MIT/Apache; ufbx MIT/Unlicense | ufbx and meshoptimizer are C/C++ built with `cc` |
| Images | `image` 0.25 (bmp, exr, hdr, jpeg, png, tga), `png` 0.18 | assets, terrain, player icon, editor thumbnails | MIT/Apache | |
| Texture cooking | `intel_tex_2` 0.5 | cook (editor only) | MIT/Apache | Prebuilt ISPC kernels; never linked by player/server/testing |
| Networking | `renet` 2, `renet_netcode` 2, `postcard` 1.1 (`use-std`) | net | MIT/Apache | renet pair moves together |
| Packing | `zstd` 0.13, `zip` 2 (deflate) | project, editor | MIT/BSD; MIT | zstd builds C with `cc` |
| UI layout | `taffy` 0.7 | ui | MIT | |
| Controllers | `gilrs` 0.11 | ui, player, editor | Apache/MIT | libudev on Linux |
| File watching | `notify` 8.2 | project | CC0 | Polling fallback |
| Parallelism, hashing | `rayon` 1.10, `rustc-hash` 2 | render, editor cooking; core and caches | MIT/Apache | |
| Profiling | `profiling` 1 (no default features) | hosts, hot functions | MIT/Apache | Tracy only with `profile-tracy`; excluded from hakari |
| Serialization | `serde` 1, `serde_json` 1 | everywhere | MIT/Apache | |
| Downloads | `ureq` 3 (rustls) | toolbox, editor | MIT/Apache | Not in the player |
| Team Create transport | `tungstenite` 0.28, `rustls` 0.23 (ring, TLS 1.3), `rcgen` 0.14, `ring` 0.17 (Ed25519 keys) | collab | MIT/Apache; rustls also ISC; ring Apache-2.0 AND ISC | Synchronous, one thread per connection |
| Utilities | `anyhow`, `thiserror` 2, `bytemuck`, `log`, `env_logger`, `pollster`, `regex`, `memory-stats` 1.2 | various | MIT/Apache | |
| MCP (editor only) | `rmcp` 3.9 (server, stdio), `tokio` 1.53 (rt, sync, io) | mcp | MIT/Apache | Never in the player or server |
| Toolbox (editor only) | `rusqlite` 0.41 (bundled SQLite), `md-5` 0.11 | toolbox | MIT | `metadata.db` |
| Release and updates | `semver` 1, `sha2` 0.11, `toml` 1, `minisign-verify` 0.2, `tar` 0.4, `flate2` 1 | release, updater | MIT/Apache | `release` is light (player and server link it); `updater` only the editor and its own binary |
| Dev only | `subsecond`, `dioxus-devtools`, `dioxus-cli-config` 0.7 | editor `hot-patch` feature | MIT/Apache | Off by default; nothing else can turn it on |

## Rules

Pin once with a comment; check Apache-2.0 compatibility; `cargo hakari generate`; keep editor-only crates out of the player ([Add a dependency](../Guides/Recipes/Add-a-Dependency.md)). The audit's package recommendations and rejected alternatives are in `docs/research/engine-modernization-report.md` sections 4.1–4.5.
