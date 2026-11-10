# `gameplay`

- Game systems moved out of `script` (roadmap L7), each a `script::GameSystem`: its services and its per-frame step. `install` adds them all; every host's `ScriptRuntime` gets them (`sim::script_runtime`). Behaviour: `docs/architecture/game-systems.md`.
- Use only `script`'s public API (`axcenity_script::system`, `LuaInstance`, `lent_world`); if a system needs more, make it public there on purpose.
- Systems step in the order `install` adds them, at Debris's old place in the script step (after content loading, before decals).
