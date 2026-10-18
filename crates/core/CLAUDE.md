# `core`

- Owns the Instance tree, generational arena, typed properties (including UI `UDim2`) and events; no engine-crate dependencies.
- See `docs/architecture.md` for data-model and crate-boundary rules. Inspect `src/world.rs` and `src/class.rs` before changing the tree or schema.
- Mutate Instances through `World` so links and change records stay consistent. Property defaults/types also drive project serialization; apply `Material` before dependent properties.
