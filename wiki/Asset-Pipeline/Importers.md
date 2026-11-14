---
title: Importers
type: system
status: current
implementation: implemented
crates: [assets, editor]
source: [crates/assets/src/import.rs, crates/assets/src/model.rs, crates/assets/src/model/fbx.rs, crates/assets/src/model/maps.rs, crates/assets/src/model/lod.rs, crates/assets/src/model/rig.rs, crates/assets/src/model/objects.rs, crates/assets/src/texture_set.rs, crates/assets/src/texture_pack.rs, crates/editor/src/app/files/import.rs]
docs: [docs/editor/import.md, docs/rendering/models.md, docs/project/assets.md]
verified: 2026-11-11
verified_commit: 19c7179d
tags: [asset-pipeline, import]
---

# Importers

> [!summary]
> Models are read at load time by `assets`: **glTF 2.0** with the `gltf` crate **FBX** (binary or ASCII, any version) and **OBJ** (with its `.mtl`, P7.1) with `ufbx`, converted to glTF's space (Y up, right-handed, meters). Every model is **fitted into the 1×1×1 cube** so `Size` stretches it like a built-in part. Materials become metallic-roughness with their maps; texture **roles are also read from file names** (`_BaseColor`, `_N`, `ORM`, …) including channel-packed layouts. LODs come from the file's `_LOD1`/`_LOD2` objects or are simplified with **meshoptimizer**. Rigs, skins and takes are read for animation. The editor's one **Import…** copies files into the project. **Implemented.**

## What the editor's import does

`EditorApp::import_with_dialog` → per kind (`EditorApp::import_file`): a model through the Import dialog (loaded first, so a bad file copies nothing), images, sounds and prefabs straight in, `.axpkg` packages through a package tab. `assets::import_model` copies a self-contained `.glb`/`.fbx` to `assets/models/<name>.<ext>`, anything with other files into `assets/models/<name>/`; **embedded textures are copied out** to `textures/` beside the model so every map is a file a SurfaceAppearance can name and the cook can cook. One undo step (`History::record_blobs`).

**Texture packs** (`assets/src/texture_pack.rs`): several maps of one material from outside the project (a Poly Haven `name_diff_2k`/`name_nor_gl_2k` download, an ambientCG `Name_2K-JPG_Color` one) are imported into `assets/textures/<pack>/` with a `<pack>.texture.json` texture set, so the Assets panel shows one tile; `import_textures` does this for Import…, dropped files and folders, and Terrain Studio's Import textures. The pack name is the file name without the trailing map word, size and format (`texture_pack_name`, using the same role words as `maps.rs`); a group needs two files filling two different maps. Loose packs already in a project are moved by the Assets panel's **Organize** (`plan_pack_folders` plus `project::plan_rename` per map, merged into one undo step), so references and `ax://` IDs survive.

## What loading reads

| | glTF | FBX |
|---|---|---|
| Geometry | Default scene's node tree (or every mesh), node transforms, triangle lists only | Every visible node with a mesh, world transforms incl. pivots; polygons triangulated by ufbx |
| Normals / tangents | Missing normals made smooth; tangents computed after fitting (file tangents ignored) | Same; ufbx makes missing normals |
| Materials | Metallic-roughness, normal, occlusion, emissive, alpha modes, `doubleSided`, `KHR_texture_transform`, `extras` (`axcenity_flutter`, `axcenity_translucency`) | ufbx's PBR mapping of Lambert/Phong/Principled, plus the material's own slots; bump maps that look like normal maps |
| Texture roles by name | Used for packed layouts | Decides roles; Lambert/Phong names win over misused slots |
| LODs | `_LOD1`/`_LOD2` objects, else meshopt to 1/2 and 1/4 of triangles (≥ 256 triangles; a copy that saves < 20% is dropped) | Same; hidden LOD objects read too |
| Collision pieces (R15.2) | `_COL`/`_COL1`… objects: one convex piece each, not drawn, `Model::collision` | Same; hidden pieces read too |
| Rig | glTF skins | Skeletons, clusters (inverse bind), takes baked at the file's frame rate |
| Not read | Vertex colors, `KHR_materials_emissive_strength`, glTF animation | Per-UV-set texture choice |

A `MeshId` can pick objects in a file (`file.fbx#A/B,C`, `model/objects.rs`). FBX textures are looked for only inside the model's folder (relative path, file name, a `textures` folder).

## Failure handling

Loading never panics on a bad file (`ModelError::Damaged`); errors name the problem; a missing or unsupported texture (`.psd`, `.dds`) doesn't stop the model, it's reported once and the material color is used.

## Related

[Pipeline overview](Pipeline-Overview.md) · [Supported formats](Supported-Formats.md) · [Animation](../Engine-Systems/Animation.md) · `docs/rendering/models.md` (the full material and map rules)
