# core

The Instance tree shared by every crate. Depends on no engine crate, and keep it that way.

- `arena.rs`: generic `Arena<T>` with generational `Id`s. A removed slot's generation goes up, so old IDs return `None`.
- `world.rs`: `World` owns all Instances. `game` (class `DataModel`) and `Workspace` are created by `World::new()` and can't be created, moved or destroyed. `Instance` fields are private; all changes go through `World` so parent and child links stay in sync.
- `class.rs`: `ClassName` and each class's `default_properties()`. A new property only needs adding there; its default value also fixes its type.
- `value.rs`: `Value`/`ValueKind`. `Color3` is sRGB 0 to 1. `Vector3` is in studs, Y up.
- `set_property` rejects unknown names and type changes. `Name` and `Parent` are fields, not entries in the property map.
- No serde here; file formats live in `project`.
