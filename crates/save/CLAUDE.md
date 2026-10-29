# `save`

- Game saves: `Saves` holds player data, settings and the active `Checkpoint`, and writes and reads numbered slot files (`slot-<n>.json`) in the player's data for the game (`game_folder`). `state.rs` snapshots and restores world state. Behavior: `docs/architecture/saves.md`.
- Depends on `core` only. Never reads or writes the project folder; hosts pass the folder (`ScriptRuntime::set_save_folder`).
- A load reads and checks the whole file before it changes the world. Restoring matches Instances by name, class and place, so scripts' references stay valid (a Tool out of place by its name and `Item`, `state::Matching`); scripts and ScreenGuis are never saved or replaced.
- Multiplayer (Phase 67): each player's data by UserId in `players/<UserId>.json` (`player_value`, `set_player_value`, `flush_players`, `forget_player`; `Drop` flushes) and their slots in `players/<UserId>/` (`for_player`).
- A slot's optional `levels` (Phase 48) is noted in `Saves::levels`; hosts with levels load them before `Saves::apply` restores the rest.
- Changing a slot file's shape raises `SAVE_VERSION` and adds a step in `migrate.rs`, with an old slot under `src/fixtures/` that a test loads.
