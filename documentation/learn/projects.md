---
description: Make, open, organize and safeguard projects: templates, the project folder, levels, version history, backups and Collaborate (editing together).
icon: folder
samples: [hello-levels]
---

# Projects

A project is a folder. Everything in it is a readable text file (except your own models, images and sounds), saved the same way every time, so projects diff cleanly and work well with Git.

## Create and open

- **New Project** (start screen, or **File > New Project…**) asks for a template, a name and a folder. **Baseplate** gives you a locked grey floor to build on, **Empty** nothing at all, and **Starter scene** a lit scene with a few blocks. The genre templates are small finished games to learn from and build on: **First Person** (a character with a blaster), **Third Person** (a course with coins and a checkpoint), **Vehicle** (a car and timed laps), **Top Down** and **Multiplayer Arena**. Each comes with a game menu and saves.
- **Open Project** (start screen, or **File > Open Project…**) opens any project folder. The start screen shows your last four projects under **Recent**, then **All projects** (up to 100, as cards or a list, with search and sort); a card's **⋯** menu (or a right-click) has Open, Settings, Version History, Show in File Manager, Remove from List (the files stay) and Delete (permanent, after you confirm).
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

## Collaborate: editing together

> **Experimental.** Collaborate (called Team Create before) works on one network, but it's new and hasn't had much use across real machines yet. Keep backups of a shared project.

Collaborate lets several people edit one project at the same time, each in their own editor.

- **Share:** open the project and choose **File > Start Collaborate Session…** (the first time, a short setup comes first: [Collaborate: set up and work together](collaborate.md)). Name the session and choose **Start Session**: your editor hosts it, shows the invite link, and the **Collaborate** panel opens. **Create invite link** makes a link for an **Editor** (can change things) or a **Viewer** (can look, but still receives the whole project). Each link works once; revoke unused ones from the panel. Links made before the rename (`axcenity://teamcreate/…`) still work.
- **Join:** paste the link into **File > Join Collaborate Session…**, or **Join…** on the start screen with no project open. Your editor fetches the project into its own folder and opens it.
- **Chat:** the **Collaborate** panel has a **Chat** for everyone in the session, Viewers included. Type a message and press Enter. Writing an object's path, such as Workspace.Door, makes it a link that selects the object; **@** puts the selected object's path in your message. New messages show as a count on the Collaborate chip in the title bar until you look at the chat. The host keeps the last 200 messages while the session runs; they're never saved in the project.
- **History:** the host keeps checkpoints of the session: when it starts, every hour with changes, and whenever an Editor names one and presses **Save** in the **Collaborate** panel's **History**. Each shows when it was made, who saved it and who changed things since the one before. The owner can restore one for everyone: the whole session goes back to it as one step, and the session as it was is kept as a checkpoint first, so the restore can be taken back the same way. Checkpoints are kept on the host's computer in the project's `.axcenity` folder.
- **Working together:** edits, undo, imports and saved files reach everyone. Others' selections are outlined in their colour with a name tag, their rows in the Explorer get a colour bar, and **Follow their camera** shows what someone is looking at. In Properties, a bar in someone's colour says which field they're dragging on the object you both have selected. Lua scripts and stylesheets are typed in together: everyone can type in the same script at once, others' typing appears as they type without moving your cursor, and **Undo** takes back only your own typing. Where someone else's cursor is in the script shows as a caret in their colour with their name, and what they've selected is shaded. The script's file is updated a couple of seconds after typing stops. Other documents (block scripts, graphs, materials, data tables, dialogues, behavior trees) are edited by one person at a time: one someone else is editing is read-only for you, with a line saying who has it.
- **Connection:** the panel shows Connected, Syncing, Reconnecting or Offline. Offline, your changes wait and are sent when the connection comes back. If the host's editor stops without ending the session, opening the project again brings the session's work back as unsaved changes.
- **Leave session** (members) or **End session** (the host) stops it. The owner sets roles and removes members in the panel.

Sessions reach other people on the same network, or from anywhere over **Tailscale**, a free private network app: when Tailscale is installed and connected on the host's computer, invites name that computer by its Tailscale name (such as `pauls-pc.tail1234.ts.net`) and the panel says so; otherwise invites work on the host's network only. Someone with their own Tailscale account joins once the host has shared their computer with them in Tailscale (admin console, **Machines › … › Share**). Tailscale only connects the computers: who may join and edit is still decided by your invites and roles. There's no Axcenity sign-in or relay service. A team can also run `axcenity-collab <project folder>` on an always-on computer, so the session doesn't depend on anyone's editor being open.

## Opening older and newer projects

When you open a project made with an older Axcenity, the editor asks **Upgrade this project?** and lists what will change. Nothing is written until you press Upgrade, and the files from before are copied to `.axcenity/previous/` first. A project saved by a newer Axcenity is refused with a message saying which version to install, and the folder is left alone.
