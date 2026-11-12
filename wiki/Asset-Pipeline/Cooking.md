---
title: Cooking
type: system
status: current
implementation: implemented
crates: [cook, assets, editor]
source: [crates/cook/src/lib.rs, crates/cook/build.rs, crates/assets/src/cooked.rs, crates/assets/src/model/cooked_model.rs, crates/assets/src/model/lod_cache.rs, crates/assets/src/asset_id.rs, crates/assets/src/orphaned.rs, crates/assets/build.rs, crates/editor/src/cooking.rs]
docs: [docs/rendering/textures.md, docs/rendering/models.md, docs/editor/assets.md, crates/cook/CLAUDE.md]
verified: 2026-11-11
verified_commit: 19c7179d
tags: [asset-pipeline, cooking, perf]
---

# Cooking

> [!summary]
> Roadmap L2, done in eight PRs on 2026-11-10, plus in-place cooking of embedded images (PR #65). The editor turns a project's source assets into GPU- and load-ready copies under the per-machine `.axcenity/imported/` folder: **textures** (offline box-filtered mips, BC7-encoded with Intel's ISPC compressor via `intel_tex_2`, normal maps as **BC5**), **simplified LODs**, and **whole cooked models**. It also gives every asset a **stable ID** in a sidecar. Copies are used only while current; a stale one is never drawn. **Implemented** ([ADR-0008](../Architecture/Architecture-Decisions/ADR-0008-Desktop-Texture-Cooking.md)).

## What gets made

| Output | Path | Made by | Contents |
|---|---|---|---|
| Cooked texture | `.axcenity/imported/textures/<file>-<path hash>.axtex` | `cook::cook_file` (editor thread or `--cook`) | `AXTEX`, version, source length + mtime + 64-bit hash, format (BC7/BC5), size, alpha flag, average color of ~4,096 samples, level lengths, levels largest first |
| LOD cache | `.axcenity/imported/meshes/<file>-<path hash>-<key>.axlod` | Any host that loads a project model (`model/lod_cache.rs`) | Each simplified primitive's copies; key hashes simplifier settings and fitted geometry |
| Cooked model | `.axcenity/imported/models/<file>-<path hash>-<pick hash>.axmodel` | Any host after a warning-free load (`model/cooked_model.rs`) | Fitted triangles, LODs, materials, skeleton; textures by origin (`ImageOrigin`) |
| Asset ID | `<asset file>.import.json` next to the file (committed) | `asset_id::give_asset_ids` | `{"format_version": 1, "uid": "ax://…"}` (and `"preset"` when one is picked, R12) (26 base-32 digits); paths remembered in `.axcenity/asset-ids.json` |

## When it runs

- On project open, a `texture cooking` thread with a rayon pool of half the cores: first `remove_orphaned_copies` (copies whose source is gone; half-written ones over an hour old), then asset IDs, then every texture without a current copy, stopping between textures if another project opens.
- After a file change the watcher reports (`asset_file_changed` → `Cooker::cook_again`).
- `axcenity-editor --cook <folder>` (exit 1 on failure); Build Game cooks what's missing first, blocking.

## Images inside a model (PR #65)

A `.glb`, a `.gltf` with `data:` images, or an `.fbx` dropped into `assets/` by hand keeps its maps inside it. After the texture files, each pass cooks those **in place** (`axcenity_cook::cook_embedded`) to `.axcenity/imported/textures/embedded-<hash of the image's bytes>.axtex`, **one copy per distinct image per project** (Abandoned Farmhouse: 144 embedded maps, 21 distinct). Each model gets a `<cache_stem>-embedded.done` note listing the copies it uses; `remove_orphaned_copies` deletes copies no note lists and now runs *after* cooking. Loading falls back to the in-place copy (`cooked_in_model`); cooked models record it as `ImageOrigin::InModel`; while an image is in use every model shares one `Image` (`shared_cooked`). Built games pack each shared copy once plus the notes. Sharing is per project only (a cooked image reads its pixels back from its own project).

## Import presets (R12)

A texture's import preset picks its compression (by name, BC7, BC5 or none) and the largest size its cooked copy keeps: four built in, more in the project's committed `import-presets.json`, with per-folder presets there and a file's own pick in its sidecar (`"preset"`). `cook::plan_for` turns the preset into a format and size; each pass recooks a copy whose format or size no longer matches and deletes the copy of an uncompressed one. The Assets panel's Import preset ▸ picks one as an undo step that recooks at once. Details: `docs/rendering/textures.md`.

## What isn't cooked

Textures whose sides aren't multiples of 4, and generated textures (built-in surfaces, terrain weights, UI images drawn at full size).

## Performance

**Measured** (cloud container unless noted): 4096² map upload 55–61 → 0.9–1.1 ms, GPU memory 85 → 21 MiB, worker read ~650 ms (JPEG decode) → ~30 ms; hello-roads cooks in 32 s on 4 cores (418 MB of copies). Quality with "very fast": PSNR 35.8 dB color, 47.4 dB roughness; normal maps were 28.1 dB as BC7 before BC5. Cooked models: CityPropsCollection.fbx 143 → 1.7–1.9 ms, Walking.fbx 178 → 19–24 ms, AWP.glb 243 → 61–63 ms. On lavapipe the first frame didn't improve (it decodes BC7 in software); a real GPU is needed to see it.

## Terrain layers

The terrain builds its texture arrays from its TerrainLayers' **pixels**, so even cooked layer maps are decoded: on the loading workers (`scene::AssetFile::LayerTexture`), with the arrays built in parallel and each texture resized once (PRs #67, #69).

## Trade-offs

- A built game ships each texture **twice** (source and cooked copy): hello-roads 393 MB → 830 MB. Dropping sources with current copies would about halve it, but the player still reads some sources' pixels ([Known bottlenecks](../Performance/Known-Bottlenecks.md)).
- `.axcenity/` is per machine: not exported, backed up or versioned; deleting it only means cooking again.

## Development guidance

- A new field on `Model`, `Primitive`, `ModelMaterial` or `Rig` must be encoded in `cooked_model.rs` (the encoder names every field, so it won't compile until handled).
- Keep the cook's mips identical to `render/src/renderer/gpu.rs`'s `half_size`.
- `player`, `server` and `testing` must never depend on `cook`.

## Related

[Pipeline overview](Pipeline-Overview.md) · `docs/rendering/textures.md` · `docs/rendering/models.md` (Cooked models, LODs) · `docs/editor/assets.md` (Asset IDs)
