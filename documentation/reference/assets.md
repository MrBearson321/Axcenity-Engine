---
description: Supported formats, what happens when a file is imported, asset IDs, cooking textures and models for the GPU, loading in the background, texture budgets, the Toolbox and licences.
icon: download
samples: [hello-mesh, hello-pbr]
---

# Assets and importing

Assets are plain files in the project folder, named by their path such as `assets/models/crate/crate.glb`. The editor imports them, gives each a stable ID, and prepares GPU-ready copies in the background. See [Assets: import and manage](../learn/assets.md) for the editor's panels.

## Supported formats

| Kind | Formats | Notes |
|---|---|---|
| Models | `.gltf`, `.glb`, `.fbx` (binary or text, any version) | 1 glTF unit or 1 metre in FBX = 1 stud. FBX files bring skeletons and animation takes. |
| Textures | `.png`, `.jpg`, `.jpeg`, `.tga`, `.bmp` | Compressed to BC7 (BC5 for normal maps) with mipmaps when cooked. |
| Sky images | `.exr`, `.hdr` (or PNG and JPEG) | Equirectangular, 2:1. HDR images let the sun be brighter than white. |
| Sounds | WAV, FLAC, Ogg Vorbis, MP3 | |
| Fonts | `.ttf`, `.otf` | |
| Colour grading | `.cube` lookup tables | For `ColorCorrectionEffect`. |
| Packages | `.axpkg` | Objects with every asset they use, from another project. |

Not supported: `.psd`, `.dds`, KTX2, USD, OBJ, and animation in glTF files. A model that names an unsupported texture loads without it and says so.

## Import

Importing copies the file into `assets/` (a model into a folder of its own, with its textures; images inside a model are copied out as files), then writes a small `<file>.import.json` beside it with the asset's **ID** (`ax://…`). Commit the sidecars with the project: they let a file be renamed or moved without breaking what uses it.

**Rename or Move…** and **Find References** in the Assets panel use these IDs and paths to update or list everything that names a file: Instances in every level, prefabs, clips, state machines, materials, data tables and scripts.

## Cooking

In the background, the editor prepares copies the GPU can use straight away, in the project's `.axcenity/imported/` folder (per computer, not committed):

- **Textures**: mipmaps, then BC7 or BC5 compression (`.axtex`).
- **Levels of detail**: simpler copies of each mesh for distance (`.axlod`).
- **Whole models**: everything a model needs in one file (`.axmodel`).

A cooked copy is used only while it matches its source; change the source and it's cooked again. **Build Game** ships the cooked copies, so players never wait for cooking.

## Loading

Assets load on background threads, so the game keeps running while a model or texture arrives. The player starts reading every asset the world uses while the splash shows, so textures don't pop in. Scripts can wait for assets with `ContentProvider:PreloadAsync()`.

Each computer has a **texture memory budget** from its graphics quality (256 MB on Low to 2 GB on Ultra): while textures fit it costs nothing; over the budget, textures are shrunk to smaller mipmap levels until they fit.

## The Toolbox and licences

The Toolbox downloads textures, skies, models and sounds from **ambientCG** and **Poly Haven** (public domain, CC0) and **Openverse** (mixed licences, filtered to CC0 and CC BY to start) into a library on your computer, then copies them into the project. Where each asset came from and its licence are kept in `.axcenity/asset-sources.json`, and Build Game writes the credits they ask for into `CREDITS.txt`, warning about anything not cleared for a commercial game.

> **Known limits.** Model files aren't watched while the editor runs: reopen the project after replacing one outside the editor. Some older projects name assets by path rather than ID; both work.

More: [Asset pipeline overview](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Asset-Pipeline/Pipeline-Overview.md) in the engineering wiki.
