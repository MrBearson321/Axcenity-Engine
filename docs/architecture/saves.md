# Saves and checkpoints

Save slots, player data, settings and checkpoints (unified plan Phase 17), and save versions (Phase 18). Code: `crates/save` (files, world state), `crates/script/src/save.rs` (`SaveService`), the save blocks in `crates/blocks`.

## SaveService

`game:GetService("SaveService")` (or `game.SaveService`) works in Scripts and LocalScripts. Both sides share one `save::Saves`, so they see the same player data, settings and checkpoint.

| Call | Does |
|---|---|
| `SaveSlot(n)` | Writes slot `n` (a whole number, 1 to 999): player data, world state, the active checkpoint and settings. Replaces what the slot held, unless a newer version of the game saved it (an error, and the slot is kept). |
| `LoadSlot(n)` | Puts the game back as slot `n` was saved. A missing, damaged or newer slot is an error (`script:line: save slot 3 doesn't exist yet; …`) and changes nothing: the file is read and checked before the world is touched. A slot from an older save version is moved up first (see Save versions). |
| `HasSlot(n)` | True if slot `n` has been saved. |
| `SetPlayerData(name, value)`, `GetPlayerData(name)` | Values kept for the player, such as coins. A boolean, number, string, Vector3, Vector2, Color3, UDim2 or EnumItem; nil removes it. Saved in slots and captured at checkpoints. |
| `SetSetting(name, value)`, `GetSetting(name)` | Game settings, such as volume, with the same values. Saved in slots, not captured at checkpoints. The names `UI/TextScale`, `UI/HighContrast` and `UI/ReducedMotion` are the game UI settings ([../ui.md](../ui.md)): they reach the world's (`World::ui_settings`, the same values as `GuiService`), nil puts one back to its default, and any other `UI/` name is an error. |
| `SetCheckpoint(checkpoint)` | Makes a `Checkpoint` the active one, and captures player data and world state now. |
| `GetCheckpoint()` | The active Checkpoint, or nil (also nil once it's destroyed). |
| `RestoreCheckpoint()` | Puts back what was captured at the active checkpoint, then stands the player character (the one whose controller owns the camera, `character::player_character`) with the middle of its feet at the checkpoint's `Position`. An error with no active checkpoint. |

Blocks (in **More**): "save game to slot *n*", "load game from slot *n*", "set checkpoint to *checkpoint*" and "go back to checkpoint", which become `SaveService:SaveSlot(n)`, `:LoadSlot(n)`, `:SetCheckpoint(workspace.Path)` and `:RestoreCheckpoint()`, with `local SaveService = game:GetService("SaveService")` at the top.

## Checkpoint

A `Checkpoint` Instance has one property, `Position`: where a restored character's feet go. It has no shape and doesn't collide. The ribbon's Effects group (beside Third Person, First Person and Agent) and Explorer Insert ▸ have **Checkpoint**, which puts it on top of the selected part, in the Workspace or the selected Model or Folder. Making one active is up to scripts (for example when the character comes near a pad, as `examples/hello-saves` does).

## World state

`save::snapshot` keeps every Instance in the Workspace, Lighting and SoundService (its SoundGroups and their effects, Phase 52), parents before children, as class, name and the properties that differ from the class defaults, plus the two services' own properties. `save::restore` matches each saved Instance to a live one with the same parent, class, name and place among same-named siblings; it sets matched ones' properties (missing ones go back to their defaults, `Material` first), makes the saved ones that are missing, and destroys live ones the save doesn't have. Matched Instances keep their `InstanceId`, so references scripts hold stay valid. A Tool (Phase 45) is matched by place first; a saved Tool with no live one in its place takes a live Tool with the same Name and `Item` from wherever it is (one the save doesn't have where it is is set aside, not destroyed), placed after the saved siblings before it, so an Inventory gets back the same Tools in hotbar order (a dropped sword returns to the Inventory as the same Instance); only Tools left over are destroyed. The Inventory's `Equipped` is a reference like any other, so the Tool in hand comes back, and the runtime fires its `Equipped`.

Each saved Instance also keeps its attributes and tags (`attributes`, `tags`), and restoring gives it exactly those, removing others. A property that points at an Instance (`ObjectValue.Value`) is saved in `references` as the number of the entry it points at (or `null` for none) and points at whatever that entry became once everything is restored; one that points outside the saved Instances isn't saved and keeps its live value. All three fields are optional, so older slots load unchanged and `SAVE_VERSION` stays 2. A slot from before SoundService was kept has no `SoundService` entry in `services`, and loading it leaves the live SoundGroups as they are. Values add `{"CFrame": [x, y, z, qx, qy, qz, qw]}` and `{"EnumItem": "KeyCode.E"}`.

Not world state, never saved, made or destroyed: scripts, Players, AnimationTracks and ScreenGuis (with everything in them). A Terrain's landscape isn't saved, only its properties. A recreated Instance doesn't get back a script that was inside it.

## Files

A slot is `slot-<n>.json` in the player's data for the game, `save::game_folder(name)`, from the project's name:

- Windows: `%APPDATA%\Axcenity\Saves\<game>`
- macOS: `~/Library/Application Support/Axcenity/Saves/<game>`
- Linux: `$XDG_DATA_HOME/axcenity/saves/<game>` or `~/.local/share/axcenity/saves/<game>`
- `AXCENITY_SAVE_DIR`, when set, replaces the base folder.

The player and editor Play set it with `ScriptRuntime::set_save_folder` before scripts start, so a script can load at the start. Editor Play uses the same folder as the built game; its tests use a temporary folder (`app/session.rs`, `play_save_folder`). Saving never writes the project folder, and Stop still drops the Play world, so the edit world and project files are as before Play. A slot is written beside itself and renamed over the old one.

The file holds `version` (`save::SAVE_VERSION`, 2), `levels` (Phase 48, only in games with levels: `{ "current": "Castle", "added": ["Dungeon"] }`, kept in `Saves::levels` by `LevelService`; older slots and slots without it load unchanged, and `LoadSlot` goes to those levels before restoring, see [../project/levels.md](../project/levels.md)), `player_data`, `settings` (with the world's game UI settings as `UI/…` and the master volume as `Audio/MasterVolume`, written on every save; loading sets the ones it has on the world and keeps them out of `Saves::settings`, so older slots load unchanged), `world` (`services` and `instances`), and `checkpoint` (`instance`, its entry in `world.instances`, with the player data and world state captured there). Values are JSON booleans, numbers and strings, or one-key objects such as `{"Vector3": [0, 5, 0]}` and `{"Material": "Neon"}`.

## Save versions

`Saves::read` (and so `LoadSlot`) reads the file's `version` first:

- **Older**: moved up to `SAVE_VERSION` one version at a time (`save::migrate`, `crates/save/src/migrate.rs`), then checked and restored like a current file. Each step works on the JSON, so old shapes need no Rust types. Loading never rewrites the file; the slot moves to the new version the next time the game saves over it. A step that can't make sense of the file makes it a damaged slot, and nothing changes.
- **Current**: read with no migration step.
- **Newer**: refused (`save slot 3 was saved by a newer version of the game …`), and `SaveSlot` won't write over it either (`… so this game won't write over it`), so an older build of the game can't lose a newer one's progress. The file is left as it was.
- **None** (no `version`, as in a project's `world.json` or `axcenity.json`): a damaged slot. Project files are never save files; project migration (`project::FORMAT_VERSION`) is separate.

| From | To | Change |
|---|---|---|
| 1 | 2 | The checkpoint was `path`, its names from the Workspace down, so two checkpoints with the same name (two inserted `Checkpoint`s) were the first one. It becomes `instance`, its entry in the slot's world state: the first saved Instance with each name in turn, as loading version 1 found it. A path that doesn't reach a saved Checkpoint drops the checkpoint, as loading version 1 did. |

To change the shape of a slot file: raise `SAVE_VERSION`, add a step to `STEPS` in `migrate.rs`, and keep a slot written by the old version under `crates/save/src/fixtures/` with a test that loads it (`slot-v1.json`).

