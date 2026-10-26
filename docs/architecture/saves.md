# Saves and checkpoints

Save slots, player data, settings and checkpoints (unified plan Phase 17). Code: `crates/save` (files, world state), `crates/script/src/save.rs` (`SaveService`), the save blocks in `crates/blocks`.

## SaveService

`game:GetService("SaveService")` (or `game.SaveService`) works in Scripts and LocalScripts. Both sides share one `save::Saves`, so they see the same player data, settings and checkpoint.

| Call | Does |
|---|---|
| `SaveSlot(n)` | Writes slot `n` (a whole number, 1 to 999): player data, world state, the active checkpoint and settings. Replaces what the slot held. |
| `LoadSlot(n)` | Puts the game back as slot `n` was saved. A missing, damaged or newer slot is an error (`script:line: save slot 3 doesn't exist yet; …`) and changes nothing: the file is read and checked before the world is touched. |
| `HasSlot(n)` | True if slot `n` has been saved. |
| `SetPlayerData(name, value)`, `GetPlayerData(name)` | Values kept for the player, such as coins. A boolean, number, string, Vector3, Vector2, Color3, UDim2 or EnumItem; nil removes it. Saved in slots and captured at checkpoints. |
| `SetSetting(name, value)`, `GetSetting(name)` | Game settings, such as volume, with the same values. Saved in slots, not captured at checkpoints. Phase 21 adds the game UI settings here. |
| `SetCheckpoint(checkpoint)` | Makes a `Checkpoint` the active one, and captures player data and world state now. |
| `GetCheckpoint()` | The active Checkpoint, or nil (also nil once it's destroyed). |
| `RestoreCheckpoint()` | Puts back what was captured at the active checkpoint, then stands the player character (the one whose controller owns the camera, `character::player_character`) with the middle of its feet at the checkpoint's `Position`. An error with no active checkpoint. |

Blocks (in **More**): "save game to slot *n*", "load game from slot *n*", "set checkpoint to *checkpoint*" and "go back to checkpoint", which become `SaveService:SaveSlot(n)`, `:LoadSlot(n)`, `:SetCheckpoint(workspace.Path)` and `:RestoreCheckpoint()`, with `local SaveService = game:GetService("SaveService")` at the top.

## Checkpoint

A `Checkpoint` Instance has one property, `Position`: where a restored character's feet go. It has no shape and doesn't collide. The ribbon's Effects group (beside Third Person, First Person and Agent) and Explorer Insert ▸ have **Checkpoint**, which puts it on top of the selected part, in the Workspace or the selected Model or Folder. Making one active is up to scripts (for example when the character comes near a pad, as `examples/hello-saves` does).

## World state

`save::snapshot` keeps every Instance in the Workspace and Lighting, parents before children, as class, name and the properties that differ from the class defaults, plus the two services' own properties. `save::restore` matches each saved Instance to a live one with the same parent, class, name and place among same-named siblings; it sets matched ones' properties (missing ones go back to their defaults, `Material` first), makes the saved ones that are missing, and destroys live ones the save doesn't have. Matched Instances keep their `InstanceId`, so references scripts hold stay valid.

Not world state, never saved, made or destroyed: scripts, Players, AnimationTracks and ScreenGuis (with everything in them). A Terrain's landscape isn't saved, only its properties. A recreated Instance doesn't get back a script that was inside it.

## Files

A slot is `slot-<n>.json` in the player's data for the game, `save::game_folder(name)`, from the project's name:

- Windows: `%APPDATA%\Axcenity\Saves\<game>`
- macOS: `~/Library/Application Support/Axcenity/Saves/<game>`
- Linux: `$XDG_DATA_HOME/axcenity/saves/<game>` or `~/.local/share/axcenity/saves/<game>`
- `AXCENITY_SAVE_DIR`, when set, replaces the base folder.

The player and editor Play set it with `ScriptRuntime::set_save_folder` before scripts start, so a script can load at the start. Editor Play uses the same folder as the built game; its tests use a temporary folder (`app/session.rs`, `play_save_folder`). Saving never writes the project folder, and Stop still drops the Play world, so the edit world and project files are as before Play. A slot is written beside itself and renamed over the old one.

The file holds `version` (`save::SAVE_VERSION`, 1), `player_data`, `settings`, `world` (`services` and `instances`), and `checkpoint` (its names from the Workspace down, with the player data and world state captured there). Values are JSON booleans, numbers and strings, or one-key objects such as `{"Vector3": [0, 5, 0]}` and `{"Material": "Neon"}`. A file whose version is newer than this engine's is refused. Migrating older versions is Phase 18.
