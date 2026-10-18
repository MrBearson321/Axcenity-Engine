# core

The Instance tree shared by every crate. Depends on no engine crate, and keep it that way.

- `arena.rs`: generic `Arena<T>` with generational `Id`s. A removed slot's generation goes up, so old IDs return `None`.
- `world.rs`: `World` owns all Instances. `game` (class `DataModel`) and its services (`Workspace`, `Players`, `ReplicatedStorage`, `ServerScriptService`, `StarterPlayer` > `StarterPlayerScripts`) are created by `World::new()` and can't be created, moved or destroyed; `World::service(class)` finds one. `Instance` fields are private; all changes go through `World` so parent and child links stay in sync.
- `World` is `Clone`: a deep copy with every Instance at the same `InstanceId` (the arena has no pointers). The editor's Play runs on one. Two copies share the free list, so both can hand out the same new ID; never mix IDs made in different copies.
- `world.rs` tree edits: `set_parent` (appends), `set_parent_at` (at a child index, for undo), `duplicate` (deep copy, detached), `is_descendant_of`.
- Change log (for scripts' `Changed`/`ChildAdded`/`ChildRemoved`): off until `record_changes`; then `set_property`, `set_name` and reparenting push numbered `Change`s (only real changes: same value, or reordering under the same parent, isn't one), read with `take_changes`. A new way to change an Instance must log its change too. `Name` and `Parent` are logged as `Change::Property`.
- `class.rs`: `ClassName` (add new classes to `ClassName::ALL`; `Model`, `Folder`, `RemoteEvent`, `Player` have no properties; `is_script()` = Script, LocalScript or ModuleScript) and each class's `default_properties()`. A new property only needs adding there; its default value also fixes its type. `events()` lists each class's own event names (Part: `Touched`), and `INSTANCE_EVENTS` the ones every class has; firing them is up to `script` and the host. `Script` has no `Source` property: its code lives in a `.server.lua` file.
- `value.rs`: `Value`/`ValueKind`. `Color3` is sRGB 0 to 1. `Vector3` is in studs, Y up. `Orientation` is degrees, applied Z, then X, then Y.
- `enums.rs`: enum property types (`PartType`, `Material`), and `KeyCode` (script input only, not a property type). A new property enum needs a `Value`/`ValueKind` variant and a case in `project`'s reader and writer.
- `set_property` rejects unknown names and type changes. `Name` and `Parent` are fields, not entries in the property map.
- Setting `Material` also resets `Metalness`/`Roughness` to `Material::preset()`. Anything that applies several properties (loading, undo, paste) must set `Material` first.
- No serde here; file formats live in `project`.
