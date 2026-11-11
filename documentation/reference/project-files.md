---
description: What's in a project folder, each file format and what it holds, the per-computer .axcenity folder, upgrading older projects, version history, backups and what a built game contains.
icon: file
samples: [hello-part, hello-levels]
---

# Project structure and file formats

A project is a folder of readable text files plus your assets, so it works well with Git and other version control. One damaged file never stops the rest of a project from opening: the editor reports it and carries on.

## The folder

| Path | Holds |
|---|---|
| `axcenity.json` | The manifest: name, description, format version, the engine version it was made and saved with, the start level, loading screen and splash settings. |
| `world.json` | The world: every Instance in the Workspace and the saved services, with only the properties that differ from their defaults. |
| `levels/<name>.world.json` | Other levels, for games with several. |
| `scripts/` | Scripts: `.server.lua`, `.client.lua`, `.module.lua`, and block scripts `.server.blocks.json` and `.client.blocks.json`. |
| `objects/` | The code of scripts that live inside objects, shared by every copy. |
| `ui/` (or anywhere) | `.css` stylesheets, and UI Event Graphs (`.uigraph.json`). |
| `assets/` | Models, textures, sounds, fonts, skies, materials (`.material.json`), texture sets (`.texture.json`), prefabs (`.prefab.json`), data tables (`.data.json`), animations, sequences, dialogues and behaviour trees. |
| `strings.json` | The string table: UI text in each language. |
| `input.json` | The input map: actions and the keys and buttons bound to them. |
| `collision-groups.json` | Collision groups and which collide. |
| `tests/` | Game tests (`.spec.lua`). |
| `.axcenity/` | Per-computer files; don't commit it (see below). |
| `thumbnail.png` | The picture on the start screen's project card. |

The full format of each file is in the engine's topic docs and the engineering wiki's [file formats](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Reference/File-Formats.md) page.

## The `.axcenity` folder

Made and used by the editor on this computer: cooked textures, levels of detail and models (`imported/`), the version history (`history/`), ignored Asset Check findings, where each Toolbox asset came from, backup settings, and the API definitions for VS Code. Everything in it but the version history is rebuilt when needed, so it can be left out of version control and backups. Build Game ships only the cooked copies from it.

## Upgrading projects

Each engine version knows the newest project format it reads. Opening a project made with an older engine asks before upgrading it; the files from before are kept in `.axcenity/previous/<version>/`. A project saved by a newer engine than yours isn't opened, so nothing is lost: update the editor instead.

## Version history and backups

- Each save records a version (up to the newest 40, plus every pinned one). **File > Version History** lists them, shows what changed in words ("Moved Part "Block" in Workspace"), and restores the whole project or one file. A restore first keeps the current files as a pinned version, so it can be undone.
- **File > Save Checkpoint…** saves a pinned version with a message.
- **File > Back Up Project** writes a dated `.zip` of the project (without `.axcenity/`) to a folder you choose, optionally on every checkpoint.

## Packages and built games

| File | Holds |
|---|---|
| `.axpkg` | An asset package: objects with every asset they use, for another project. |
| `game.axgame` | A built game: the manifest, world, levels, string table, scripts (blocks translated to Lua), stylesheets and assets with their cooked copies, compressed. Editor-only files stay out. |

## Player data

Built games keep their saves, settings and logs in the player's data folder, never in the project: save slots (`slot-<n>.json`), per-player data, graphics settings (`graphics.json`) and `logs/latest.log` (with the four runs before).
