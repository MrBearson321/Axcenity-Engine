# `player`

- Runtime binary shipped with games. Frame order: `docs/architecture/frame-loop.md`. Crate rules: `docs/architecture/crates.md`. For script behavior, open one file from `docs/scripting.md`.
- Never add dependencies on `editor`, `egui` or `blocks` (block scripts and graphs arrive translated through `project` and `blocks-runtime`). Preserve the ordered frame steps and report loop errors through the existing error path.
- `input.rs`: winit's input as `InputEvent`s, the game's held input for the controllers (input map actions), and the cursor `MouseBehavior` asks for; tests build an `App::new` without a window (`input_tests.rs`).
- Levels (Phase 48): `game_levels` hands every level to the runtime before scripts start; `follow_level_loads` shows the loading screen again (`level_tests.rs`).
- Networking (Phase 63): `--host [port]`, `--join address:port` and `--password` (parsed in `watch.rs`'s `Arguments`); `App::network` receives first in the frame and sends just before rendering (`docs/architecture/networking.md`).
- `controller_input`: the controllers' input, none while a TextBox is typed in or a Sequence locks input (Phase 51, `cutscene_tests.rs`).
- Keep platform-specific behavior guarded; builds target Windows, macOS and Linux.
