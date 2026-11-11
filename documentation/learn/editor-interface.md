---
description: The window, the ribbon, the panels and how to arrange them, the camera, search, settings and plugins.
icon: layout
samples: [hello-part, hello-plugins]
---

# Editor interface

The editor is one window: a header with the menus, a **ribbon** of tools under it, and a docking area of **panels** below. Everything you can click is also reachable from the keyboard through search.

## The header and menus

- **File**: New Project, Open Project, Save, Version History, Save Checkpoint, Back Up Project, Input, Close Project, Import, **Build Game**, stylesheets, Check Assets, Set Up VS Code Autocomplete, Settings and Quit.
- **Edit**: undo and redo, rename, duplicate and delete, **Search Everything**, the Command Palette, Quick Open and Find in Scripts.
- **View**: the editor's own windows: **Panels** (show or hide any panel), **Reset Layout**, **Layout** presets and **New Ribbon Tab…**.
- **Viewport**: what the Scene draws over the world while you edit: the guides (the measuring **Grid**, the **View Cube**, **Light Cones** and the **Movement Guide**, the line and distance shown while you move a part), **View Mode** (debug views), **Overlays** (colliders, raycasts, bounds, sound emitters, nav mesh, agent paths, constraints, culling) and **Viewport Settings…**.
- **Debug**: **Graphics Benchmark…**, which measures how this computer draws the viewport and recommends the editor's own graphics settings (never your game's).
- **Help**: **Learn** (this page, the Documentation and the API Reference in a window beside your project), the Version Explorer, Check for Updates and About.

The title in the middle shows the project's name, with a `*` while there are unsaved changes.

## The ribbon

The ribbon's tabs group the tools by task:

| Tab | What's on it |
|---|---|
| **Home** | Play and Stop, the Select, Move, Resize, Rotate and Transform tools, snapping, the part shapes, and the Panels menu |
| **Model** | Paint, Material, Anchor, Lighting, Emission and Angular; solid modeling (Union, Negate, Intersect, Separate); constraints; Import |
| **World** | Lights and sky, effects (sound, particles, post-processing), gameplay objects (characters, prompts, spawns, vehicles) and terrain |
| **UI** | Screens, layouts, UI elements and controls for your game's interface |
| **Script** | New Script, LocalScript and ModuleScript, and script tools |
| **Test** | Play with several clients, network conditions, Check Assets, and view options such as overlays |

Press **+** after the tabs to make your own tab from any of the ribbon's groups; right-click it to edit or delete it. With the **Compact** density setting the ribbon shows icons only, with names in tooltips.

## Panels

The **Scene** (the 3D viewport), **Explorer**, **Properties**, **Console**, **Assets**, **Toolbox**, **Animation**, **UI Designer**, **Terrain**, **Profiler**, **Mixer** and every open script are tabs in one docking area:

- Drag a tab onto another panel's tabs to join them, onto a panel's edge to split it, or outside to float it in its own window (on any monitor). **Dock Back** in a floating window puts its panels back.
- Close a panel with its **×** and bring it back from **View > Panels**. **View > Reset Layout** puts everything where it started.
- **View > Layout** switches between presets for a task: Default, Scripting, Animation, UI Design, Modeling & Assets, and Lighting & Environment. **Save Current Layout…** keeps your own.

The layout is remembered between runs.

## The camera

| Do this | To |
|---|---|
| Hold the right mouse button and move | Look around |
| **W A S D**, **E** and **Q** (with the mouse over the Scene) | Fly, and go up and down |
| Hold **Shift** | Fly slowly |
| Mouse wheel | Zoom toward what you're looking at |
| Middle button drag | Pan |
| **Alt** and left drag | Orbit |
| **Shift** and the wheel, or the wheel while right-dragging | Change the fly speed (1 to 500 studs a second) |

The **view cube** in the Scene's corner shows which way you face; click a face, edge or corner to look from there.

## Search everything

Press **Ctrl+K** (**Cmd+K** on macOS) to search the whole editor at once: commands, tools, objects in the world, assets, scripts, levels, settings and the selected object's properties. Chips under the search box narrow it to one kind. **Ctrl+Shift+P** opens it on commands only (the Command Palette) and **Ctrl+P** is Quick Open. Every menu item and ribbon button can be run from here, with its shortcut shown.

## Settings

**File > Settings…** (**Ctrl+,**, or the gear) has two groups. **Editor** settings are yours, saved on this computer for every project:

- **Appearance**: dark, light or system theme, accent colour and interface scale. (The theme is no longer in the View menu.)
- **Text & density**: Comfortable or Compact, and the code font size.
- **Editor layout**: panels, layout presets and your saved layouts, and the start screen's navigation.
- **Viewport**: the grid and its strength, the view cube, light cones, the movement guide, the view mode and the overlays: the same switches as the Viewport menu.
- **Graphics**: the viewport's quality preset, shadows, post-processing, render scale, texture quality and anti-aliasing for this computer. These change how the editor draws, not your game's own Lighting.
- **Shortcuts**: every keyboard shortcut, grouped by where it works.
- **Accessibility**: Reduce motion, Increase contrast and the steady frame rate.
- **Updates**: the update channel and checks, on official builds.

**Project** settings (**This project**) are saved in the project folder and shared with everyone who opens it: the project's name and folder, **Input…**, **Collision Groups…**, and the Workspace and Lighting settings.

## Plugins

> **Experimental.** Plugins work, but their API may still change.

Plugins are Lua scripts that add tools to the editor: buttons on the ribbon's **Plugins** tab and panels of their own built from UI objects. They run only while you edit, never in a built game.

- A plugin is a folder with a `plugin.json` (its name and version) and an `init.lua`, either in the project's `plugins/` folder (for that project) or in the editor's own **Plugins** folder (for every project). Plugins reload when you save their files.
- The **Plugins** panel (**View > Panels**) lists them, with an on/off box, **Reload** and any errors. **New Plugin…** makes a starter plugin to edit.
- **Export** on a plugin saves it as one `.axcenity-plugin` file to send to someone. **Import plugin…**, in the Plugins panel or the Toolbox's **Packs** menu, installs one. A package may only hold `plugin.json`, `.lua` and `.css` files.
- A plugin can only read and write the project's files if its `plugin.json` asks for the `files` permission.

The [hello-plugins](https://github.com/MrBearson321/Axcenity-Engine/tree/main/examples/hello-plugins) sample has two worked plugins.

## Keyboard shortcuts you'll use most

| Keys | Do |
|---|---|
| **1** to **5** | Select, Move, Resize, Rotate, Transform |
| **Escape** | Back to Select |
| **F5** / **Shift+F5** | Play / Stop |
| **Ctrl+S** | Save |
| **Ctrl+Z** / **Ctrl+Y** or **Ctrl+Shift+Z** | Undo / Redo |
| **Ctrl+D** | Duplicate |
| **F2** | Rename |
| **Delete** | Delete (Backspace too on macOS) |
| **Ctrl+K** | Search everything |
| **F9** | The developer console, during Play |

On macOS, Cmd takes the place of Ctrl. Settings > Shortcuts lists them all.
