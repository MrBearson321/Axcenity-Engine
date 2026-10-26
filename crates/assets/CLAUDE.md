# `assets`

- Loads project asset files (glTF and FBX models for now) into plain data, and copies imported models into a project. Paths: `docs/project/assets.md`. Meshes and maps: `docs/rendering/models.md`.
- No engine-crate dependencies; `scene` turns models into draw items and `physics` into hulls. Read files only through here, inside the project folder (`safe_path`). `Assets::fonts` reads the project's `.ttf`/`.otf` files once (`forget_fonts` rereads).
- Loading must never panic on a bad file: `load_model` turns `gltf` and `ufbx` panics into `ModelError::Damaged`, and each failure is reported once (`Assets::take_errors`). FBX is read from memory only (`load_external_files: false`); its textures are found by `fbx::texture_path`, inside the folder.
- A `MeshId` can pick objects in its file (`file.fbx#A/B,C`, `model/objects.rs`); glTF and FBX must build object paths the same way, with `Siblings`.
- What a texture is for, by file name, and channel-packed layouts: `model/maps.rs` (`role_from_name`, `ModelMaterial::set_map`). A texture set groups those maps in one file: `texture_set.rs`.
- Skeletons, takes and CPU skinning: `model/rig.rs`; `model/test_rig.rs` generates the rigged example FBX (`AXCENITY_WRITE_EXAMPLES=1` rewrites it). In FBX clusters, `Transform` is the inverse bind.
