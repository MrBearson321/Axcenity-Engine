# player

Binary that ships with exported games. Must never depend on `editor`, `egui` or `blocks`.

- `main.rs`: takes a project folder path as its first argument. winit 0.30 `ApplicationHandler`; the window is created in `resumed`, not before the loop starts. Errors in the loop go through `App::fail` and are returned from `main`.
- `scene.rs`: turns the `World` into a `render::DrawList`. This is the only code that sees both Instances and render types. Lighting and the camera are fixed constants here until the Lighting service and camera controls exist. The editor will need the same step in Phase 8; move it into a shared crate then rather than copying it.
- wgpu is pinned to 25.x (used through `render`). Its API changes between majors, so check the changelog when upgrading.
