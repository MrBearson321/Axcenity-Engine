# Axcenity Engine

*Axcenity* is pronounced **ax-SEN-ih-tee**.

A free, open-source 3D game engine and editor inspired by Roblox Studio. Build worlds from parts, script them in **Lua 5.4** or with **Scratch-style blocks**, press Play to test, and export standalone games for **Windows, macOS and Linux**. Written in Rust.

> **Status:** early development (Phase 6 of the [v1 plan](plans/axcenity-engine-v1.md)). Right now the player loads a project folder, runs its Lua scripts (which can pause with `task.wait`), simulates physics (unanchored parts fall and collide), and draws its parts (blocks, balls, cylinders and wedges in plastic, metal, glass or neon), and you can fly around the scene.

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

Camera controls: hold the **right mouse button** and move the mouse to look around. **W A S D** move, **E** and **Q** go up and down, and holding **Shift** moves slowly.

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
