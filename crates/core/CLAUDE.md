# `core`

- Owns the Instance tree, generational arena, typed properties (including UI `UDim2`) and events, `TerrainData` (heights, paint, foliage) and the world's `StringTable`, active language and game `UiSettings` (`ui_settings.rs`, saved in slots as `UI/…` settings); no engine-crate dependencies.
- Boundaries: `docs/architecture/crates.md`. Instances: `docs/architecture/data-model.md`. Inspect `src/world.rs` and `src/class.rs` before changing the tree or schema.
- `World::copy_from` copies a subtree from another world (levels load this way), leaving scripts out. `prefab.rs` (Phase 49): a placed copy's `PrefabLink`, its overrides (children matched by name and occurrence) and `sync`; no file access (that's `project::Prefabs`).
- Mutate Instances through `World` so links and change records stay consistent. Property defaults/types also drive project serialization; apply `Material` before dependent properties.
- `tool.rs` (Phase 45): where a Tool is (`carried`: in hand or put away in an Inventory), which physics, scene, character, ui and script all ask.
- `input.rs`: the input map (`input.json`'s actions), `InputEvent`s and the held `InputState` every host and the script runtime share.
- `ThirdPersonController` and `FirstPersonController` are declared here (shared properties: `movement_properties`). The `character` crate steps it; do not make `core` depend on `character`.
