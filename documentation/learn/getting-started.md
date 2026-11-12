---
description: What Axcenity is, how to install it, and your first five minutes from New Project to Play.
icon: rocket
samples: [hello-part, hello-touched]
---

# Getting started

Axcenity is a free, open-source 3D game engine and editor. You build worlds out of parts and models, script them in **Luau** (a fast, typed dialect of Lua) or with **blocks**, press **Play** to test, and build a standalone game for Windows, macOS or Linux. The scripting API will feel familiar if you have used Roblox Studio: objects are Instances in one tree, `workspace` holds the 3D world, and scripts react to events such as `Part.Touched`.

> Axcenity is in **alpha** (version 0.2). Everything described in Learn exists and is tested, but file formats and some APIs can still change between releases. Pages mark anything that is only partly built or experimental.

## Install

- **Installers**: alpha builds are on the project's [GitHub releases page](https://github.com/MrBearson321/Axcenity-Engine/releases): a Windows setup wizard (`.exe`), a macOS disk image (`.dmg`), and Linux `.deb`, `.rpm`, `.pkg.tar.zst`, `.tar.gz` and AppImage packages.
- **From source**: install Rust with rustup, then run the editor from the engine's folder:

```sh
cargo run -p axcenity-editor
```

Windows and Linux are the main platforms and are checked on every change. macOS works but is checked less often. Mobile and web are not supported.

## The start screen

With no project open, the editor shows the start screen. Its sidebar has:

- **Projects**: templates to start from, your **Recent** projects, then **All projects** (search, sort, or switch to a list), **New Project**, **Open Project** and **Join…** for a Team Create session.
- **Global Assets**: models, textures and sounds you keep for every project.
- **Learn**: this learning path, and short step-by-step tutorials.
- **Documentation**: the technical reference, with search.
- **API Reference**: every class, property, event and function scripts can use.
- **Settings**: theme, accent colour, density, motion and graphics.

## Your first project

1. Press **New Project**. Pick a template: **Baseplate** (a grey floor), **Empty**, **Starter scene**, or a playable genre template (**First Person**, **Third Person**, **Vehicle**, **Top Down**, **Multiplayer Arena**). Give it a name and a folder, then create it.
2. The project opens. The 3D **Scene** is in the middle, the **Explorer** (every object in the game) and **Properties** (the selected object's settings) are on the right, and the **Console** is at the bottom.
3. On the **Home** tab of the ribbon, press **Block**. A new part appears in front of the camera and is selected.
4. Press **2** for the Move tool and drag an arrow to move the block. **3** resizes and **4** rotates. **1** goes back to Select.
5. Press **F5** (or **Play**) to run the game. Scripts run, anything they `print` appears in the Console, and your block falls: new parts are unanchored. Tick **Anchored** in Properties to hold a part in place. Press **Shift+F5** (or **Stop**) and the world goes back exactly as it was before Play.
6. Press **Ctrl+S** (**Cmd+S** on macOS) to save. A project is a folder of readable text files, so it works well with Git.

## Moving the camera

Hold the **right mouse button** and move the mouse to look around. While it is held, **W A S D** fly, **E** and **Q** go up and down, and **Shift** slows down. The mouse wheel zooms, the middle button pans, and **Alt** with the left button orbits. **Shift** and the wheel change how fast you fly.

## Where to go next

- Take the tutorial [Place and move a part](https://github.com/MrBearson321/Axcenity-Engine/tree/main/examples/hello-part) in the **hello-part** sample, then [Editor interface](editor-interface.md).
- Every sample project in the engine's `examples` folder can be opened from Learn with one click. They run without the editor too.
- When you want the details behind a feature, the [Documentation](../README.md) has a page for each system.
