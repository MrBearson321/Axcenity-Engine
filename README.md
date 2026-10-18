# Axcenity Engine

*Axcenity* is pronounced **ax-SEN-ih-tee**.

A free, open-source 3D game engine and editor inspired by Roblox Studio. Build worlds from parts, script them in **Lua 5.4** or with **Scratch-style blocks**, press Play to test, and export standalone games for **Windows, macOS and Linux**. Written in Rust.

> **Status:** early development (Phase 16 of the [v1 plan](plans/axcenity-engine-v1.md)). Right now the player loads a project folder, runs its Lua scripts (which can pause with `task.wait` and react to parts touching with `part.Touched:Connect`), simulates physics (unanchored parts fall and collide), and draws its parts (blocks, balls, cylinders and wedges in plastic, metal, glass or neon), and you can fly around the scene. The first version of the editor opens a project and shows it in a 3D viewport, with an Explorer tree, a Properties panel and an Output panel. You can select parts, move, scale and rotate them with handles (with grid snapping), edit their properties, insert new parts, Models and Folders, rename, duplicate and delete them, drag them around the Explorer to regroup them, undo and redo all of it, and save the project. **Play** runs the game right in the editor, and **Stop** puts the world back exactly as it was. Scripts open in tabs with Lua syntax highlighting (double-click one in the Explorer, or press **Script** in the toolbar to make a new one). Edits made in VS Code show up within a second, clicking a script error in Output jumps to its line, and a damaged file is reported and skipped instead of stopping the whole project from opening. Games are split into a server side (`Script`s) and a player's side (`LocalScript`s, which can read the keyboard and move the camera), which talk through `RemoteEvent`s, as in Roblox. Shared code goes in `ModuleScript`s that scripts load with `require`, and a script stuck in an endless loop is stopped after 3 seconds with an error instead of freezing everything. Scripts can react to changes (`part.Changed`, `workspace.ChildAdded`) and run code every frame (`RunService.Heartbeat`), and **File > Set Up VS Code Autocomplete** makes VS Code autocomplete the engine's API in your scripts.

## Getting started

1. Install Rust with [rustup](https://rustup.rs). On Windows, rustup also asks you to install the Microsoft C++ Build Tools; accept that.
2. From this folder, run the player with the example project:

   ```bash
   cargo run -p axcenity-player -- examples/hello-part
   ```

   Try editing `examples/hello-part/world.json` (for example the block's `Color`, `Metalness` or `Roughness`) and running it again. For more log output, set `RUST_LOG=info` first.

3. For every shape and material, run `examples/primitives` instead.
4. For Lua scripting, run `examples/hello-script`. Its `scripts/build.server.lua` moves the red block and builds a row of glowing balls, and anything it `print`s appears in the terminal. Every `*.server.lua` file in a project's `scripts` folder starts when the game starts; a script error shows the file and line, and the other scripts keep running.
5. For timed scripts, run `examples/hello-tasks`. The lamp switches between red and blue every second (`task.wait(1)` in a loop), the bar spins (`task.spawn`), and a ball appears after 3 seconds (`task.delay`), while the terminal prints `blink 1`, `blink 2`, and so on.
6. For physics, run `examples/hello-physics`. Parts fall, stack and roll down a ramp onto an anchored floor. After 2 seconds `scripts/drop.server.lua` releases the red block (`Anchored = false`), freezes it in mid-air (`Anchored = true`), then lets it drop onto the tower, and `scripts/reset.server.lua` teleports the green ball back up every 5 seconds. Parts are unanchored by default, so add `"Anchored": true` to floors and walls in your own `world.json`.
7. For events, run `examples/hello-touched` (the Milestone 1 demo). A red ball drops onto a pad; `scripts/pad.server.lua` connects to `pad.Touched`, so each landing changes the pad's color, makes it glow for half a second, and prints `Ball touched the pad` (a bounce counts as a second landing). A blue ball drops at 3 seconds; at 6 seconds the script calls `:Disconnect()`, and the last ball lands without any reaction.

8. For the server/client split, run `examples/hello-remote` and hold the **arrow keys**: the blue block slides around. `scripts/keys.client.lua` is a LocalScript (it runs on the player's side, so it can read the keyboard with `UserInputService`) and sends the direction through the `MoveRequest` RemoteEvent in `ReplicatedStorage`; `scripts/mover.server.lua` is a Script (it runs on the server) and moves the block when the message arrives. Every `*.client.lua` file becomes a LocalScript.
9. For shared code, run `examples/hello-modules`: a staircase of colored steps appears, and the terminal prints `Built 8 steps with the Builder module`. `scripts/shared/Palette.module.lua` and `Builder.module.lua` are ModuleScripts (they appear in `ReplicatedStorage`); `scripts/stairs.server.lua` loads one with `require(game.ReplicatedStorage.Builder)`, which in turn requires `Palette`. Every `*.module.lua` file becomes a ModuleScript.
10. For change events, run `examples/hello-events`. The blue bar spins (`RunService.Heartbeat` runs every frame), the terminal prints `the ball reached the floor` (the ball's `GetPropertyChangedSignal("Position")` fires as physics moves it), `Crate was added to the Workspace` (`workspace.ChildAdded`) and `Lamp's Color changed` (`lamp.Changed`).

Camera controls: hold the **right mouse button** and move the mouse to look around. **W A S D** move, **E** and **Q** go up and down, and holding **Shift** moves slowly.

### The editor

```bash
cargo run -p axcenity-editor -- examples/hello-touched
```

The editor shows the project in a 3D viewport, the **Explorer** (right) lists every object from `Game` down (click the arrows to open and close branches), and the **Output** panel (bottom) shows messages, load errors, and what scripts print. Use **File > Open Project…** to open another project folder.

**Selecting and editing:** click a part in the viewport, or a row in the Explorer, to select it. It gets a blue box in the viewport and its row is highlighted. The **Properties** panel (under the Explorer) shows its properties: drag a number sideways to scrub it, or click it to type a value, and pick colors, materials and shapes from their controls. Changes show in the viewport immediately. **File > Save** (Ctrl+S, or Cmd+S on macOS) writes `world.json`. While there are unsaved changes the title shows a `*`, and closing or opening another project asks whether to save first.

**Moving, scaling and rotating:** pick a tool in the toolbar under the menu bar, or press **Ctrl+1** (Select), **Ctrl+2** (Move), **Ctrl+3** (Scale) or **Ctrl+4** (Rotate) (Cmd on macOS). The selected part gets handles: drag a **Move** arrow to slide it along that axis, a **Scale** ball to push or pull that face (the opposite face stays put), or a **Rotate** ring to turn it. Handles turn yellow under the mouse. **Snap move** (studs, also used by Scale) and **Snap rotate** (degrees) in the toolbar can be switched off, and their step sizes changed by dragging or clicking the number.

**Dragging parts:** with any tool, drag a part by its body (anywhere but a handle) to move it: it slides over whatever is under the mouse and rests on it, so you can drop it on the floor, stack it on another part or push it against a wall. With **Snap move** on, it lands on the 1-stud grid (or whatever step you set).

**Building the tree:** the **Block**, **Sphere**, **Cylinder** and **Wedge** buttons at the right of the toolbar insert a part at the point the camera is looking at. Right-click a row in the Explorer for **Insert** (parts, **Model**, **Folder**), **Rename**, **Duplicate** and **Delete**. New things go inside the selected Model or Folder, or into the Workspace otherwise, and get a name no sibling has (`Part`, `Part2`, …). Shortcuts: **F2** renames (Enter keeps it, Escape cancels), **Ctrl+D** (Cmd+D) duplicates, **Delete** deletes. Drag a row in the Explorer onto another row to move it inside that one; rows you can't drop on show a "not allowed" cursor and the reason. Services and scripts can't be moved or copied, nothing can be put inside a script, and services can't be deleted.

**Play and Stop:** press **▶ Play** at the left of the toolbar (or **F5**) to run the game in the viewport: scripts run, parts fall, and `print` output and script errors appear in Output. If there are unsaved changes, Play saves them first. While the game plays, the Explorer and Properties show the live world (click things to inspect them). You can change properties in the Properties panel, and the game's scripts see the change (`Changed` fires), but those changes are undone at Stop; everything else is off until Stop. **■ Stop** (or **Shift+F5**) ends the game and puts every object and property back as it was before Play. Play runs each script file as it is on disk at that moment, so you can edit scripts in another program and press Play again. If the engine itself crashes during Play, Output says so and the editor goes back to editing instead of closing; in debug builds, **Debug > Panic during Play** triggers this on purpose.

**Scripts:** double-click a Script in the Explorer (under ServerScriptService) to open it in a tab above the viewport; the **World** tab goes back to the 3D view. Press **Script** in the toolbar (or right-click > Insert > Script) to make a new one: it creates `scripts/Script.server.lua` and opens it. **Ctrl+S** saves the world and every changed script (unsaved tabs show a ●), and Play saves them first too. Rename a script with **F2** in the Explorer (this renames its file); deleting one asks first, because it deletes the file. You can also edit scripts in VS Code while the editor is open: saved changes show up in the editor within a second. When a script error appears in Output, click it to open the script at that line. A damaged file (such as a broken `world.json`) is reported in Output and skipped, and the rest of the project still opens; if you save over a world file the editor couldn't load, the old one is kept as `world.backup.json`.

**Server and client:** the **LocalScript** button (or right-click > Insert > LocalScript) makes `scripts/LocalScript.client.lua`, a script that runs on the player's side, in **StarterPlayer > StarterPlayerScripts**. Right-click > Insert > **RemoteEvent** adds a RemoteEvent to **ReplicatedStorage**, which is saved in `world.json` like the Workspace. If a script uses something from the wrong side, such as `UserInputService` (the keyboard) in a Script, a **⚠** appears next to it in the Explorer (and on a closed folder holding it) and on its tab; hover it, or open the script, to see why and what to do instead. During Play, the keyboard goes to LocalScripts (except while you type in a text box).

**Modules and the watchdog:** the **ModuleScript** button (or right-click > Insert > ModuleScript) makes `scripts/ModuleScript.module.lua` in **ReplicatedStorage**. Put functions in its table, and use them from any Script or LocalScript with `local Utils = require(game.ReplicatedStorage.ModuleScript)`. If a script runs for 3 seconds without pausing (for example `while true do end`, a loop with no `task.wait()`), the editor freezes for those 3 seconds, then the script is stopped, Output shows the error with a link to the line, and the rest of the game keeps running.

**Autocomplete in VS Code:** **File > Set Up VS Code Autocomplete** adds the engine's API to the project for the Lua extension (Lua Language Server): `.axcenity/axcenity.lua`, `.luarc.json` and `.vscode/extensions.json`. Open the project folder in VS Code and install the Lua extension when it asks; typing `Instance.new(`, `part.` or `task.` then lists what's there, and mistakes such as `part.Size = 5` are underlined. When you open the project with a newer editor, the definitions are updated.

**Undo and redo:** **Ctrl+Z** undoes and **Ctrl+Y** (or Ctrl+Shift+Z) redoes, also under the **Edit** menu, which names the step ("Undo Move Pad"). A whole drag or a typed name is one step. Saving keeps the history, so you can still undo after saving.

In the viewport, the player's camera controls work while the mouse is over it, plus: the **mouse wheel** zooms, **middle-drag** pans, and **Alt + left-drag** orbits around the point you're looking at. To fly faster or slower, scroll with **Shift** held, or scroll while holding the **right mouse button**; the new speed pops up in the middle of the viewport.

Useful commands:

| Task | Command |
|---|---|
| Build everything | `cargo build --workspace` |
| Run tests | `cargo test --workspace` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Format | `cargo fmt --all` |

On Linux you may need your distribution's Vulkan driver package (for example `mesa-vulkan-drivers`).

## Project layout

- `crates/`: the engine, split into focused crates (see [docs/architecture.md](docs/architecture.md))
- `examples/`: example project folders
- `assets/branding/`: the engine's logo and icons
- `definitions/`: the Lua Language Server definitions for the script API (generated)
- `docs/`: design notes per subsystem
- `prd/`: the product requirements for v1
- `plans/`: the phase-by-phase implementation plan

## License

Axcenity Engine is licensed under the [Apache License 2.0](LICENSE).

- **Your games are yours.** Games you make with Axcenity belong entirely to you: no royalties, no fees, sell or share them however you like. Exported games include a credits file with the engine's [NOTICE](NOTICE); the export tool adds it for you.
- **Forks are welcome.** You may fork and modify the engine freely. Keep the `LICENSE` and `NOTICE` files, mark files you changed, and give your fork its own name. The Apache License does not grant use of the Axcenity name.
