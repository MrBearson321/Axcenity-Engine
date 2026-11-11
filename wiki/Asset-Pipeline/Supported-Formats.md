---
title: Supported formats
type: reference
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [Cargo.toml, crates/assets/src/lib.rs, crates/assets/src/sky_image.rs, crates/assets/src/fonts.rs]
docs: [docs/project/assets.md, docs/rendering/models.md, docs/rendering/sky.md, docs/editor/assets.md]
tags: [asset-pipeline, reference]
---

# Supported formats

Source asset formats the engine reads. Project data files (JSON) are in [File formats](../Reference/File-Formats.md).

| Kind | Extensions | Read by | Notes |
|---|---|---|---|
| Models | `.gltf`, `.glb` | `gltf` 1.4 (`KHR_texture_transform`, `extras`) | Embedded and `data:` images allowed |
| Models | `.fbx` (binary, ASCII, any version) | `ufbx` 0.11 (vendored C) | Read from memory only; external files must be inside the model's folder |
| Textures | `.png`, `.jpg`/`.jpeg`, `.tga`, `.bmp` | `image` 0.25 | Decoded to RGBA8 unless a current cooked copy exists |
| HDR sky images | `.exr`, `.hdr` | `image` (`exr`, `hdr` features) | Equirectangular 2:1; scaled down past 4096 wide. PNG/JPEG skies work but can't be brighter than white |
| Sounds | what kira's decoders accept | `kira` 0.12 | Through `Assets::read_file` |
| Fonts | `.ttf`, `.otf` | `glyphon` / cosmic-text | `Assets::fonts`, read once |
| Color grading LUTs | `.cube`, strip images | `assets/src/color_grading.rs` | Lighting's `ColorCorrectionEffect` |
| Packages | `.axpkg` | `zip` 2 | Asset packages between projects |
| Cooked (generated) | `.axtex`, `.axlod`, `.axmodel` | `assets` | [Cooking](Cooking.md) |
| Built games | `.axgame` (v1, v2) | `project/src/packed.rs` | v2: zstd per entry, mtimes |

**Not supported**: `.psd`, `.dds`, KTX2 (a model naming one loads without that texture and says so), USD, OBJ, glTF animation (FBX takes are the animation source), audio formats kira can't decode.

Downloads: the editor's [Toolbox](../Editor/Toolbox.md) fetches assets from ambientCG, Poly Haven and Openverse into a central asset directory and copies them into the project with provenance; the player never links it.
