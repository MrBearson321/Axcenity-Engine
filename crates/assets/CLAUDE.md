# `assets`

- Loads project asset files (glTF models for now) into plain data, and copies imported models into a project; see `docs/project-and-export.md` (Assets) and `docs/rendering.md` (Imported models).
- No engine-crate dependencies; `scene` turns models into draw items and `physics` into hulls. Read files only through here, inside the project folder (`safe_path`).
- Loading must never panic on a bad file: `load_model` turns `gltf` panics into `ModelError::Damaged`, and each failure is reported once (`Assets::take_errors`).
