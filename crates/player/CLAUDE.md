# `player`

- Runtime binary shipped with games. Frame order: `docs/architecture/frame-loop.md`. `preload.rs` (I11) reads the Workspace's assets ahead and holds the splash or loading screen until they're in (`docs/project/export.md`). Crate rules: `docs/architecture/crates.md`. For script behavior, open one file from `docs/scripting.md`.
- Never add dependencies on `editor`, `egui` or `blocks` (block scripts and graphs arrive translated through `project` and `blocks-runtime`). Preserve the ordered frame steps and report loop errors through the existing error path.
- `input.rs`: winit's input as `InputEvent`s, the game's held input for the controllers (input map actions), and the cursor `MouseBehavior` asks for; tests build an `App::new` without a window (`input_tests.rs`). `controller_input` gives none while a TextBox is typed in or a Sequence locks input (`cutscene_tests.rs`).
- Levels (Phase 48): `game_levels` hands every level to the runtime before scripts start; `follow_level_loads` shows the loading screen again (`level_tests.rs`).
- Networking (Phase 63): `--host [port]`, `--join address:port` and `--password` (parsed in `watch.rs`'s `Arguments`); `App::network` receives first in the frame and sends just before rendering (`docs/architecture/networking.md`).
- Developer Console and logs (Phase 71): `dev_console.rs` (F9, commands, stats), `logs.rs` (`latest.log`, panic hook). Print with `logs::line`, not `println!` (`docs/developer-console.md`).
- Keep platform-specific behavior guarded; builds target Windows, macOS and Linux.
