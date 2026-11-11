---
description: Make, open, organize and safeguard projects: templates, the project folder, levels, version history and backups.
icon: folder
samples: [hello-levels]
---

# Projects

A project is a folder. Everything in it is a readable text file (except your own models, images and sounds), saved the same way every time, so projects diff cleanly and work well with Git.

## Create and open

- **New Project** (start screen, or **File > New Project…**) asks for a template, a name and a folder. **Baseplate** gives you a locked grey floor to build on, **Empty** nothing at all, and **Starter scene** a lit scene with a few blocks. The genre templates are small finished games to learn from and build on: **First Person** (a character with a blaster), **Third Person** (a course with coins and a checkpoint), **Vehicle** (a car and timed laps), **Top Down** and **Multiplayer Arena**. Each comes with a game menu and saves.
- **Open Project** (start screen, or **File > Open Project…**) opens any project folder. The start screen keeps up to 100 recent projects as cards; a card's **⋯** menu (or a right-click) has Open, Settings, Version History, Show in File Manager, Remove from List (the files stay) and Delete (permanent, after you confirm).
- From a terminal, `axcenity-editor <folder>` opens a project directly.

Opening reads the project in the background with a progress screen, so the window never freezes, and a damaged file is reported in the Console and skipped instead of stopping the whole project from opening.

## What's in the folder

| Path | Holds |
|---|---|
| `axcenity.json` | The manifest: name, description, format version, start level, loading screen and splash |
| `world.json` | Every object in the Workspace and the services (Lighting, ReplicatedStorage, StarterPlayer, …) |
| `scripts/` | Lua scripts: `*.server.lua` (Scripts), `*.client.lua` (LocalScripts), `*.module.lua` (ModuleScripts) and block scripts (`*.blocks.json`) |
| `objects/` | Scripts that live inside objects, such as a door's own script |
| `assets/` | Models, textures, materials, sounds, animations, prefabs and data tables |
| `levels/` | Extra levels, one `*.world.json` each |
| `ui/` | Stylesheets (`*.css`) and UI Event Graphs |
| `tests/` | Game tests (`*.spec.lua`) |
| `.axcenity/` | Per-computer files: cooked textures, version history, editor state. Don't commit it |

The [project structure reference](../reference/project-files.md) lists every file.

## Project settings

On the start screen, a project card's **Settings** changes its name, description, whether built games show the Axcenity splash, and which ScreenGui to use as a loading screen instead. **File > Input…** edits the project's input actions (such as Jump or Reload) and their keys and controller buttons.

## Levels

A game can have several levels. `world.json` is the start level; every other level is a file in `levels/`. The **Levels** panel adds, opens and renames them. A level is its Workspace: Lighting, ReplicatedStorage, StarterPlayer and the other services belong to the project and stay the same from level to level. Scripts switch levels with `LevelService:LoadLevel("Castle")`. The **hello-levels** sample has three levels (Meadow, Castle and Dungeon) joined by portals.

## Version history, checkpoints and backups

- Every save also keeps a version of the project in `.axcenity/history` (the newest 40, plus any you pin). **File > Version History…** lists them with what changed in words ("Moved Part Block in Workspace"), and can restore the whole project or one file. Restoring keeps a "Before restore" version first, so it can be undone.
- **File > Save Checkpoint…** saves with a message and pins that version, so it's kept however many saves follow.
- **File > Back Up Project** writes a dated `.zip` of the whole project to a backup folder you choose the first time.

## Opening older and newer projects

When you open a project made with an older Axcenity, the editor asks **Upgrade this project?** and lists what will change. Nothing is written until you press Upgrade, and the files from before are copied to `.axcenity/previous/` first. A project saved by a newer Axcenity is refused with a message saying which version to install, and the folder is left alone.
