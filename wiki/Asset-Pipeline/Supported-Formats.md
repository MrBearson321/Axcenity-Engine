---
title: Supported formats
type: reference
status: current
verified: 2026-11-14
verified_commit: 19c7179d
source: [Cargo.toml, crates/assets/src/lib.rs, crates/convert/src/lib.rs, crates/assets/src/sky_image.rs, crates/assets/src/fonts.rs]
docs: [docs/project/assets.md, docs/rendering/models.md, docs/rendering/sky.md, docs/editor/assets.md]
tags: [asset-pipeline, reference]
---

# Supported formats

Source asset formats the engine reads. Project data files (JSON) are in [File formats](../Reference/File-Formats.md).

| Kind | Extensions | Read by | Notes |
|---|---|---|---|
| Models | `.gltf`, `.glb` | `gltf` 1.4 (`KHR_texture_transform`, `extras`) | Embedded and `data:` images allowed |
| Models | `.fbx` (binary, ASCII, any version) | `ufbx` 0.11 (vendored C) | Read from memory only; external files must be inside the model's folder |
| Models | `.obj` + `.mtl` | `ufbx` 0.11 (`FileFormat::Obj`) | P7.1: the `mtllib` file is read from the model's folder and passed in; Y up, 1 unit = 1 stud; not cooked to `.axmodel` while it has a library |
| Models (converted) | `.usd`, `.usda`, `.usdc`, `.usdz` | `openusd` 0.8 + `openusd-schemas` 0.8 (pure Rust), editor only (`convert`) | P7.2, [ADR-0027](../Architecture/Architecture-Decisions/ADR-0027-Convert-New-Model-Formats-To-glTF-On-Import.md): converted to `.glb` before the Import dialog; composed stage, units and up axis, UsdPreviewSurface materials; no UsdSkel yet |
| Textures | `.png`, `.jpg`/`.jpeg`, `.tga`, `.bmp` | `image` 0.25 | Decoded to RGBA8 unless a current cooked copy exists |
| HDR sky images | `.exr`, `.hdr` | `image` (`exr`, `hdr` features) | Equirectangular 2:1; scaled down past 4096 wide. PNG/JPEG skies work but can't be brighter than white |
| Sounds | what kira's decoders accept | `kira` 0.12 | Through `Assets::read_file` |
| Fonts | `.ttf`, `.otf` | `glyphon` / cosmic-text | `Assets::fonts`, read once |
| Color grading LUTs | `.cube`, strip images | `assets/src/color_grading.rs` | Lighting's `ColorCorrectionEffect` |
| Packages | `.axpkg` | `zip` 2 | Asset packages between projects |
| Cooked (generated) | `.axtex`, `.axlod`, `.axmodel` | `assets` | [Cooking](Cooking.md) |
| Built games | `.axgame` (v1, v2) | `project/src/packed.rs` | v2: zstd per entry, mtimes |

**Not supported**: `.psd`, `.dds`, KTX2 (a model naming one loads without that texture and says so), Alembic (deferred, see ADR-0027), STL, PLY, glTF morph-weight animation, audio formats kira can't decode.

Downloads: the editor's [Toolbox](../Editor/Toolbox.md) fetches assets from ambientCG, Poly Haven and Openverse into a central asset directory and copies them into the project with provenance; the player never links it.
