# Licensing and branding

The engine license, credits, and logo files.

- The engine is **Apache-2.0**. The `NOTICE` reads: "Built with Axcenity Engine, © 2026 Paul Janota and contributors."
- Every new dependency must be license-compatible with Apache-2.0 (MIT, Apache-2.0, BSD, Zlib and similar are fine; no GPL). Add its notice to the generated credits.
- Games made with the engine belong to their creators, with no royalties.

- `assets/branding/` holds the engine's logo, supplied by the developer. See its `README.md` for which file is for what. GitHub release installers (NSIS wizard, macOS disk image) draw their pictures from those files via `packaging/generate-assets.py`.
- The editor and the player use `icon-256.png` as their window icon, built in with `include_bytes!` (the player decodes it with the `png` crate, the editor with eframe). Windows and X11 show window icons; macOS and Wayland ignore them, so the app bundle and desktop file icons come with export (`icon.ico` and `icon.png` are ready for it).
- The wordmark (added after Phase 16) comes in a light-background and a dark-background version, both made from `wordmark-source.png` by `assets/branding/build.py`. The editor shows it on its start screen and About box; it's ready for splash and loading screens. The window icon is now the hexagon mark on a white rounded square.
- Fonts (`assets/fonts/`, SIL Open Font License 1.1, compatible with Apache-2.0 for bundling) and icons (`egui-phosphor`, MIT) are credited in the editor's About box and must go in the generated credits once the editor is shipped.
- glTF loading (Phase 17) uses `gltf` and `gltf-json` (MIT OR Apache-2.0) and, for textures, `image` (MIT OR Apache-2.0) with its PNG, JPEG, TGA and BMP decoders. FBX loading uses `ufbx` (the Rust bindings and the bundled C library, MIT OR Unlicense; we take it under MIT). All of them go in the generated credits.
