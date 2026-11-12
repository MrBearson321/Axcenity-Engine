# Templates

New Project's playable templates (unified Phase 69): First Person, Third Person, Vehicle, Top Down and Multiplayer Arena, beside Baseplate and Empty (the Starter scene went in unified Phase 92: the genre templates replace it; projects made from it are unaffected). Code: `crates/project/src/templates.rs` (`Template`, `create_project_from`), `crates/project/build.rs`, and the editor's `start.rs` and `start/dialogs.rs`.

## Where they live

- Each is a folder in the repository's `templates/` (`first-person`, `third-person`, `vehicle`, `top-down`, `arena`). `templates/shared/` holds what every one has: the `GameMenu` prefab (`assets/prefabs/GameMenu.prefab.json`), `menu.css`, `scripts/GameMenu.client.lua`, `GameSaves.server.lua`, `LocalGui.module.lua`, `MenuSettings.module.lua` and `tests/menu.spec.lua`.
- `build.rs` embeds every file under `templates/<folder>/` in the `project` crate (`TEMPLATE_FILES`), so the editor writes them anywhere. Files at the top of `templates/` and hidden files are left out.
- `create_project_from` writes the new manifest and an empty `world.json`, then the shared files, then the template's own; a template's file with the same path as a shared one replaces it (the Arena's `GameSaves.server.lua` and `tests/menu.spec.lua`, each template's `MenuSettings.module.lua`).
- A template folder has no `axcenity.json`, so it isn't a project by itself. To change a template, make a project from it, edit that, and copy the changed files back.
- Each world places the menu as a linked prefab copy with nothing inside (`{"class": "ScreenGui", "name": "GameMenu", "prefab": "assets/prefabs/GameMenu.prefab.json"}`); loading fills it in from the prefab (`docs/project/world.md`, Prefabs). The editor writes it in full on its first save.

## What each has

| Template | The game | Blocks |
|---|---|---|
| First Person | A FirstPersonController character with an Inventory holding the Blaster Tool (attributes `Damage`, `ClipSize`, `Ammo`). `Blaster.client.lua` sends each shot along the camera through the `Blaster` RemoteEvent; `Blaster.server.lua` raycasts and damages a target's Health. `Reload` is an action in `input.json` (R). Targets are Models tagged `Target`; `Targets.server.lua` counts them as player data. | `MovingTarget.server.blocks.json` tweens a target from side to side. |
| Third Person | The default StarterCharacter (a Humanoid), spawned on a SpawnLocation. `StarterPlayer.CharacterFootPlacement` is on, so a skinned character given to the template plants its feet. `Course.server.lua`: coins (player data, `leaderstats.Coins`), the green pad's Checkpoint, and a fall puts you back on it, or respawns you before it. | `Door.server.blocks.json` slides the door open when its ProximityPrompt is triggered. |
| Vehicle | A car (Insert ▸ Vehicle's) whose VehicleSeat is Disabled, so its `DrivePrompt` seats you (`seat:Sit`). `Race.server.lua` times laps through `Gates` in order, by game time, with medals from `assets/data/medals.data.json` (`Seconds`, `Title`, `Tint`); the best lap is player data. | `Seat.server.blocks.json` turns the prompt off when someone sits. |
| Top Down | The Hero has a ThirdPersonController (so it's the player's character that enemies sense) and a NavigationAgent. `ClickToMove.client.lua` holds a Scriptable camera above it and sends the point under a click through the `MoveTo` RemoteEvent; `Hero.server.lua` sets the agent's Destination, hurts the Hero near an enemy and counts a win at the Goal. Enemies run `assets/ai/Guard` and `Patrol.behaviortree.json`. | `Alarm.server.blocks.json` turns an enemy red when it sees you. |
| Multiplayer Arena | Red and Blue Teams, a SpawnLocation each, `RespawnTime` 3. `Arena.server.lua` gives each new character the Sword from ServerStorage, hits the other team and the `TrainingDummy` within `Reach`, and keeps `leaderstats` Kills and Deaths (`ArenaStats.module.lua`, per-player data on the server) with knockouts in the chat. | `Welcome.server.blocks.json` shows a system message at the start. |

Each has a `Hud` ScreenGui and `hud.css`, and comments starting "START HERE" in the script to change first.

## The menu and the save slot

- **Pause**: Menu (Escape) shows `GameMenu.Pause` and sets the character's controller `PlatformStand`, so it stands still. Resume, Settings, Save and Load.
- **Settings**: a row per action in `MenuSettings.module.lua`'s `Actions`, copied from the hidden `Rows.Row`; clicking one and pressing a key, mouse button or controller button calls `UserInputService:RebindAction`.
- **Save and Load** call the `SaveGame` RemoteFunction that `GameSaves.server.lua` makes: `SaveService:SaveSlot(1)` and `LoadSlot(1)` (slot from `MenuSettings`), then the `GameLoaded` BindableEvent so scripts put their HUD numbers back. The Arena's keeps only the player's Kills and Deaths in their per-player data, since loading the world mid-match would move everyone.
- **Each device's own UI**: ScreenGuis are in the Workspace, which a host shares with every player, so `LocalGui.claim(name)` gives the first device to ask the ScreenGui itself (its `Owner` attribute) and every later device a copy of its own, switching the original off there. Single-player always gets the original. Both the menu and the HUDs use it.

## Your own templates

Any project can become a template (`.axtemplate`) that New Project lists beside the built-in ones. Code: `crates/project/src/user_templates.rs` (`save_template`, `read_template`, `list_templates`, `create_project_from_template_file`), the editor's `app/save_template.rs` (File > Save as Template…) and `start/dialogs.rs` (Your templates).

- **File > Save as Template…** (also in Ctrl+K) asks for a name and a description, saves unsaved changes first, and writes `<name>.axtemplate` into the editor's templates folder: `Templates/` in the editor's data folder, beside its settings (`EditorApp::default_templates_folder`). A template of the same name is replaced, and the dialog says so first. Characters file systems don't allow in a name become `-`.
- **The file** is a zip: `axtemplate.json` (`format` 1, `name`, `description`, the engine version that made it), the project's `thumbnail.png` as its picture when it has one, and every project file under `project/`. `.axcenity/` (this computer's cache, version history and AutoRecover) and a Collaborate project's `.git/` and `collaborate.json` (its repository, remote and project ID) are left out. A newer `format`, a zip without `axtemplate.json`, or a path that would climb out of the project is refused.
- **New Project > Your templates** lists every `.axtemplate` in that folder, by name, with its picture (or a placeholder) and description. **Import…** copies a template file someone gave you into the folder (as `Name 2.axtemplate` when the name is taken); the folder icon shows the folder. A file there that isn't a template is left out of the list.
- **Making a project** from one unpacks its files into the new folder (refusing a folder that exists, and removing what it wrote if it fails), then gives the manifest the new project's name and this engine's version. The new project starts with its own empty history. It doesn't get the default sky unless the template had it.
- To share a template, send its `.axtemplate` file; it holds only project files, never settings or keys from the computer that made it.

## Pictures

`thumbnail.png` (320 × 180) in each template folder is its picture on the start page ("Or from a game") and in the New Project dialog (`Template::picture`), and the new project's first thumbnail. They're drawn from the game's own camera a second in (the Vehicle and Arena from above) by `cargo test -p axcenity-player write_template_pictures -- --ignored`; run it after changing a template's world.

## Tests

- `crates/testing/tests/templates.rs`: a project from each template loads without problems and passes its `tests/*.spec.lua` through `Runner` and the `axcenity-test` binary, with no errors or warnings in the output.
- `crates/editor/src/app/template_tests.rs`: each plays in the editor without errors, and the Tests panel gives the command line's results.
- `crates/player/src/template_tests.rs`: each plays as a built game (`pack_game`, `unpack_game`) without errors, and the Arena hosted on a UDP port with two players joined keeps their teams, characters, menus (the host's pause doesn't open theirs), chat and knockouts.
- `crates/project/src/user_templates.rs`: a saved template makes projects with the same world and scripts under their own name, without `.axcenity/`, `.git/` or `collaborate.json`; the template isn't packed into itself; non-templates, newer formats and climbing paths are refused without leaving a folder.
- `crates/editor/src/app/save_template.rs`: Save as Template saves unsaved changes first and a project made from it has them; the dialog draws and Escape closes it. `crates/editor/src/start/tests.rs`: New Project lists your templates, imports one and makes a project from it.
- `crates/project/src/tests.rs`: every template loads, has the shared files and a block script; a template's own file replaces the shared one; each picture is 320 × 180.
