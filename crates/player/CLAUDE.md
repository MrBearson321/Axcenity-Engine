# player

Binary that ships with exported games. Must never depend on `editor`, `egui` or `blocks`.

- `main.rs`: takes a project folder path as its first argument. winit 0.30 `ApplicationHandler`; the window is created in `resumed`, not before the loop starts. Errors in the loop go through `App::fail` and are returned from `main`.
- `main.rs` starts every script after loading (Scripts, then LocalScripts), before the window opens (`start_scripts`). Frame step 1 sends the camera (`camera_state`) and the frame's `KeyEvent`s (winit keys → `SCRIPT_KEYS`; focus loss releases held keys) to scripts; a LocalScript's camera move is applied before drawing. `App` keeps the `ScriptRuntime` and `Physics` and steps scripts, then physics, then fires `Touched` (`take_touches` → `fire_touched`) each frame (tests do the same with `run_until`). `print` goes to stdout, errors to stderr as `error: path:line: message`.
- The frame (`RedrawRequested`) follows the fixed order in `docs/architecture.md`, one numbered comment per step. Game time is the capped frame time (`MAX_FRAME_SECONDS`), shared by camera and scripts.
- The draw list and the camera come from the `scene` crate (shared with the editor). `set_move_key` maps winit `KeyCode`s onto `scene::MoveKeys`. Mouse look uses raw `DeviceEvent::MouseMotion` while the right button is held.
- wgpu is pinned to 25.x (used through `render`). Its API changes between majors, so check the changelog when upgrading.
