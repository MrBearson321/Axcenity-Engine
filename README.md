# Axcenity Engine

*Axcenity* is pronounced **ax-SEN-ih-tee**.

A free, open-source 3D game engine and editor inspired by Roblox Studio. Build worlds from parts, script them in **Lua 5.4** or with **Scratch-style blocks**, press Play to test, and export standalone games for **Windows, macOS and Linux**. Written in Rust.

> **Status:** early development (Phase 10 of the [v1 plan](plans/axcenity-engine-v1.md)). Right now the player loads a project folder, runs its Lua scripts (which can pause with `task.wait` and react to parts touching with `part.Touched:Connect`), simulates physics (unanchored parts fall and collide), and draws its parts (blocks, balls, cylinders and wedges in plastic, metal, glass or neon), and you can fly around the scene. The first version of the editor opens a project and shows it in a 3D viewport, with an Explorer tree, a Properties panel and an Output panel. You can select parts, move, scale and rotate them with handles (with grid snapping), edit their properties, undo and redo, and save the project; adding and deleting parts comes next.

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

Camera controls: hold the **right mouse button** and move the mouse to look around. **W A S D** move, **E** and **Q** go up and down, and holding **Shift** moves slowly.

### The editor

```bash
cargo run -p axcenity-editor -- examples/hello-touched
```

The editor shows the project in a 3D viewport, the **Explorer** (right) lists every object from `Game` down (click the arrows to open and close branches), and the **Output** panel (bottom) shows messages and load errors. Use **File > Open Project…** to open another project folder. Scripts don't run in the editor yet; Play comes in a later phase.

**Selecting and editing:** click a part in the viewport, or a row in the Explorer, to select it. It gets a blue box in the viewport and its row is highlighted. The **Properties** panel (under the Explorer) shows its properties: drag a number sideways to scrub it, or click it to type a value, and pick colors, materials and shapes from their controls. Changes show in the viewport immediately. **File > Save** (Ctrl+S, or Cmd+S on macOS) writes `world.json`. While there are unsaved changes the title shows a `*`, and closing or opening another project asks whether to save first.

**Moving, scaling and rotating:** pick a tool in the toolbar under the menu bar, or press **Ctrl+1** (Select), **Ctrl+2** (Move), **Ctrl+3** (Scale) or **Ctrl+4** (Rotate) (Cmd on macOS). The selected part gets handles: drag a **Move** arrow to slide it along that axis, a **Scale** ball to push or pull that face (the opposite face stays put), or a **Rotate** ring to turn it. Handles turn yellow under the mouse. **Snap move** (studs, also used by Scale) and **Snap rotate** (degrees) in the toolbar can be switched off, and their step sizes changed by dragging or clicking the number.

**Dragging parts:** with any tool, drag a part by its body (anywhere but a handle) to move it: it slides over whatever is under the mouse and rests on it, so you can drop it on the floor, stack it on another part or push it against a wall. With **Snap move** on, it lands on the 1-stud grid (or whatever step you set).

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
- `docs/`: design notes per subsystem
- `prd/`: the product requirements for v1
- `plans/`: the phase-by-phase implementation plan

## License

Axcenity Engine is licensed under the [Apache License 2.0](LICENSE).

- **Your games are yours.** Games you make with Axcenity belong entirely to you: no royalties, no fees, sell or share them however you like. Exported games include a credits file with the engine's [NOTICE](NOTICE); the export tool adds it for you.
- **Forks are welcome.** You may fork and modify the engine freely. Keep the `LICENSE` and `NOTICE` files, mark files you changed, and give your fork its own name. The Apache License does not grant use of the Axcenity name.
