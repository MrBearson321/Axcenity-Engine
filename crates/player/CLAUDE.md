# player

Binary that ships with exported games. Must never depend on `editor`, `egui` or `blocks`.

- `main.rs`: takes a project folder path as its first argument. winit 0.30 `ApplicationHandler`; the window is created in `resumed`, not before the loop starts. Errors in the loop go through `App::fail` and are returned from `main`.
- `main.rs` starts every server script after loading, before the window opens (`start_scripts`); `App` keeps the `ScriptRuntime` and `Physics` and steps scripts, then physics, then fires `Touched` (`take_touches` → `fire_touched`) each frame (tests do the same with `run_until`). `print` goes to stdout, errors to stderr as `error: path:line: message`.
- The frame (`RedrawRequested`) follows the fixed order in `docs/architecture.md`, one numbered comment per step. Game time is the capped frame time (`MAX_FRAME_SECONDS`), shared by camera and scripts.
- `scene.rs`: turns the `World` into a `render::DrawList`. This is the only code that sees both Instances and render types. It applies the Part shape rules (balls and cylinders stay round) and the material looks (glass opacity, neon glow). Lighting is fixed constants until the Lighting service exists. The editor will need the same step in Phase 8; move it into a shared crate then rather than copying it.
- `camera.rs`: `FlyCamera` (yaw/pitch, movement scaled by frame time) and `MoveKeys`. No winit window code, so it's unit-tested. Mouse look uses raw `DeviceEvent::MouseMotion` while the right button is held.
- wgpu is pinned to 25.x (used through `render`). Its API changes between majors, so check the changelog when upgrading.
