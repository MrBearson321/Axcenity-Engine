# `server`

- The `axcenity-server` binary (Phase 63): loads a game, runs its Scripts and physics headless at 60 Hz (no window, audio or GPU), and hosts it with `net::Network` (no player of its own). Docs: `docs/architecture/networking.md`.
- Never add `winit`, `scene`, `ui`, `player` or anything that opens a window, a GPU device or audio output: it must start on a Linux machine with no display. `cargo tree -p axcenity-server -e normal -i wgpu` (and `kira`, `winit`) must find nothing: `script` takes render data from `render-types` and sounds from `core::sound` (roadmap M8), and hakari leaves `winit` out of the workspace hack.
- Keep its frame in the frame order (`docs/architecture/frame-loop.md`), network receive first and network send last. `tests/headless.rs` runs the real binary.
