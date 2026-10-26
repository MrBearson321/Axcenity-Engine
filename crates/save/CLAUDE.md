# `save`

- Game saves: `Saves` holds player data, settings and the active `Checkpoint`, and writes and reads numbered slot files (`slot-<n>.json`) in the player's data for the game (`game_folder`). `state.rs` snapshots and restores world state. Behavior: `docs/architecture/saves.md`.
- Depends on `core` only. Never reads or writes the project folder; hosts pass the folder (`ScriptRuntime::set_save_folder`).
- A load reads and checks the whole file before it changes the world. Restoring matches Instances by name, class and place, so scripts' references stay valid; scripts and ScreenGuis are never saved or replaced.
