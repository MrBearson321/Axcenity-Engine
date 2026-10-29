# `server`

- The `axcenity-server` binary (Phase 63): loads a game, runs its Scripts and physics headless at 60 Hz (no window, audio or GPU), and hosts it with `net::Network` (no player of its own). Docs: `docs/architecture/networking.md`.
- Never add `winit`, `scene`, `ui`, `player` or anything that opens a window, a GPU device or audio output: it must start on a Linux machine with no display. (`script` links `render` and `audio` as plain data.)
- Keep its frame in the frame order (`docs/architecture/frame-loop.md`), network receive first and network send last. `tests/headless.rs` runs the real binary.
