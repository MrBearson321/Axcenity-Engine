---
description: The window, the ribbon, the panels and how to arrange them, the camera, search and settings.
icon: layout
samples: [hello-part]
---

# Editor interface

The editor is one window: a header with the menus, a **ribbon** of tools under it, and a docking area of **panels** below. Everything you can click is also reachable from the keyboard through search.

## The header and menus

- **File**: New Project, Open Project, Save, Version History, Save Checkpoint, Back Up Project, Input, Close Project, Import, **Build Game**, stylesheets, Check Assets, Set Up VS Code Autocomplete, Settings and Quit.
- **Edit**: undo and redo, rename, duplicate and delete, **Search Everything**, the Command Palette, Quick Open and Find in Scripts.
- **View**: theme, **Panels** (show or hide any panel), **Reset Layout**, **Layout** presets, Light Cones, the View Cube, the measuring **Grid**, **View Mode** (debug views) and **Overlays** (colliders, raycasts, bounds, sound emitters, nav mesh, agent paths, constraints, culling).
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

**File > Settings…** (**Ctrl+,**) has:

- **Appearance**: dark, light or system theme, accent colour and interface scale.
- **Text & density**: Comfortable or Compact, and the code font size.
- **Editor layout**: panels, layouts, the grid and the overlays.
- **Graphics**: the viewport's quality preset, shadows, post-processing, render scale, texture quality and anti-aliasing for this computer. These change how the editor draws, not your game's own Lighting.
- **Shortcuts**: every keyboard shortcut, grouped by where it works.
- **Accessibility**: Reduce motion, Increase contrast and the steady frame rate.
- **Updates**: the update channel and checks, on official builds.

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
