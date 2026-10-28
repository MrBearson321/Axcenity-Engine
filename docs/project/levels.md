# Levels

A game's levels (unified Phase 48). Code: `crates/project/src/levels.rs` (files), `crates/script/src/levels.rs` (`LevelService`), `crates/editor/src/levels.rs` and `app/levels.rs` (the Levels panel). Example: `examples/hello-levels`.

## Files

- `world.json` is the **start level**: the game starts there. Its name is the manifest's optional `start_level` (`"start_level": "Meadow"` in `axcenity.json`), `Start` when it has none. A project with only `world.json` has that one level and opens and plays as before.
- Every other level is `levels/<name>.world.json`, a world file holding only its `workspace` (`project::write_level`). The folder is optional, so the format version is unchanged.
- A level is its Workspace. ReplicatedStorage, Lighting, Players, StarterPlayer, Teams and SoundService are the project's, saved in `world.json`, and stay the same from level to level. A level file's other sections are left unused.
- Level names follow script names' file name rules (`project::check_level_name`), and differ from every other level's and the start level's, ignoring capitals.
- File > Build Game packs `levels/` with the rest (`project::pack_game`), so a built game has every level.

## LevelService

`game:GetService("LevelService")` (or `game.LevelService`), on both sides. Hosts hand every level over with `ScriptRuntime::set_levels(start, current, worlds)`: the player reads `levels/` at start (`project::load_levels`), and the editor's Play does the same after saving, with the open level's world from the editor.

| Member | Does |
|---|---|
| `CurrentLevel` | The level in the Workspace now: the start level, until one loads (in the editor's Play, the open level). |
| `LoadLevel(name, spawnName?)` | Puts the level's Workspace in place of the current one: the Workspace's properties and children, without scripts. The Workspace's ScreenGuis (the game's UI) and the players' characters stay; a ScreenGui of the level's with the name of one that stayed isn't brought in. Levels added with `AddLevel` go. Each character then stands on the Instance called `spawnName` in the new level (the middle of the top of a part, or a Checkpoint's `Position`), or, without one, on one of their SpawnLocations chosen at random; a `spawnName` the level doesn't have warns in Output. An unknown level is an error naming the levels, and changes nothing. |
| `LevelLoaded(name)` | Fires on both sides once the level is in, after the changes the load made have fired. Handlers connected to Instances of the old level are gone with them, so scripts connect to the new level's here. |
| `AddLevel(name)` | Copies the level into a new Model of that name in the Workspace, beside the current level, and returns it. An error if it's added already. |
| `RemoveLevel(name)` | Takes an added level's Model out (it's destroyed). An error if it isn't added. |

Scripts live in the project's services, so they keep running through a load. The loading screen (the project's `loading_screen` ScreenGui, File > Build Game) shows again for its minimum time after each load, in the player and the editor's Play (`ScriptRuntime::take_level_loads`); without one, nothing shows, as the load is immediate.

Save slots keep the current level and the added ones (`levels` in the slot, see [../architecture/saves.md](../architecture/saves.md)). `SaveService:LoadSlot` goes to the slot's level first if it's another one (firing `LevelLoaded`), adds and removes levels to match, then restores the rest over them, so loading from a main menu in the start level returns to the level the slot was saved in.

Blocks (in **More**): "load level *name* at *spawn*", "add level *name*" and "remove level *name*", which become `LevelService:LoadLevel(name, spawn)` (just `(name)` when *spawn* is empty), `:AddLevel(name)` and `:RemoveLevel(name)`, with `local LevelService = game:GetService("LevelService")` at the top.

## The editor

View > Panels > **Levels** lists the start level (a flag) and each level in `levels/`, with the open one in the accent colour. Its rows:

- **Open** (or double-click): asks about unsaved changes first ("Save changes?"), then opens the project again with that level in the Workspace (`project::load_with_level`), its terrains loaded and the UI Event Graphs of its ScreenGuis compiled. The undo history starts again. Saving then writes the Workspace to the level's file and the services to `world.json`, whose Workspace stays the start level's (`project::save_level`). A level file the editor couldn't load is kept as `levels/<name>.backup.json` before it's saved over. While another level is open, `world.json` changed elsewhere isn't reloaded; opening the start level reads it.
- **New Level** adds an empty level (the Baseplate template's Workspace) called `Level` (or `Level 2`…), **Duplicate** copies a level's saved Workspace to `<name> copy`, and both start renaming it. **Rename** renames a level's file, or, for the start level, only `start_level`. **Delete** removes a level's file. Each is one undo step (`History::record_asset`, the files' texts before and after). The open level can't be renamed or deleted, nor can the start level be deleted.
- **Set as start level** saves the project, then moves that level's Workspace into `world.json` and the old start level's into `levels/<old name>.world.json`, and sets `start_level`, as one undo step (`project::set_start_level`). Whichever level is open stays open, in its new file.

Play starts in the open level. **Play from start level** in the panel makes it start in the start level instead (the open level is still in `LevelService`'s levels, as saved by Play). Asset Check checks every level: the open one as the world, the others read from their files (`project::check_assets_in_level`), their findings at `Site::Level` (the file and the place among its Workspace's children), which Clear, Use and Replace fix in the file.
