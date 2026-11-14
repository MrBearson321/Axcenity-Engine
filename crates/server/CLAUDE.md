# `server`

- The `axcenity-server` binary (Phase 63): loads a game, runs its Scripts and physics headless at 60 Hz (no window, audio or GPU), and hosts it with `net::Network` (no player of its own). Docs: `docs/architecture/networking.md`.
- Never add `winit`, `scene`, `ui`, `player` or anything that opens a window, a GPU device or audio output: it must start on a Linux machine with no display. `cargo tree -p axcenity-server -e normal -i wgpu` (and `kira`, `winit`) must find nothing: `script` takes render data from `render-types` and sounds from `core::sound` (roadmap M8), and hakari leaves `winit` out of the workspace hack.
- `axcenity-bots` (`src/bots/`, multiplayer readiness MP1) is headless players for scale tests: the player's frame steps with no window, over real UDP (`--churn`, `--flood` for MP2's operational tests, `tests/operations.rs`); it has the same no-window, no-GPU rule. `--stats` is `src/stats.rs`.
- Its frame is the shared schedule's server steps (`axcenity_sim`, `docs/architecture/frame-loop.md`), with the network and console output as its `HostIo`. `tests/headless.rs` runs the real binary.
