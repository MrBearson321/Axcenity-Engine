# player

Binary that ships with exported games. Must never depend on `editor`, `egui` or `blocks`.

- `main.rs`: takes a project folder path as its first argument. winit 0.30 `ApplicationHandler`; the window is created in `resumed`, not before the loop starts. Errors in the loop go through `App::fail` and are returned from `main`.
- `main.rs` runs every server script once after loading, before the window opens (`run_scripts`): `print` goes to stdout, errors to stderr as `error: path:line: message`. Phase 5 moves the `ScriptRuntime` into `App` so the scheduler can resume scripts each frame.
- `scene.rs`: turns the `World` into a `render::DrawList`. This is the only code that sees both Instances and render types. It applies the Part shape rules (balls and cylinders stay round) and the material looks (glass opacity, neon glow). Lighting is fixed constants until the Lighting service exists. The editor will need the same step in Phase 8; move it into a shared crate then rather than copying it.
- `camera.rs`: `FlyCamera` (yaw/pitch, movement scaled by frame time) and `MoveKeys`. No winit window code, so it's unit-tested. Mouse look uses raw `DeviceEvent::MouseMotion` while the right button is held.
- wgpu is pinned to 25.x (used through `render`). Its API changes between majors, so check the changelog when upgrading.
