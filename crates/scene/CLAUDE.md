# scene

The bridge between Instances and the renderer, shared by `player` and `editor`. Depends only on `core` and `render`; no winit or egui here.

- `draw_list.rs`: `build_draw_list(world, camera)` turns Workspace Parts into a `render::DrawList`. This is the only code that sees both Instances and render types. It applies the Part shape rules (balls and cylinders stay round) and the material looks (glass opacity, neon glow). Lighting is fixed constants until the Lighting service exists.
- `camera.rs`: `FlyCamera` (yaw/pitch plus a focus point `focus_distance` ahead) and `MoveKeys` (plain booleans). `look`, `update` (fly at `speed`, scaled by frame time), `change_speed`/`set_speed` (×1.2 per step, 1–500 studs/s), and the editor's `orbit`, `pan` and `zoom`. Each app maps its own input events onto these: the player from winit `KeyCode`s, the editor from egui keys.
- `pick.rs`: `CameraView` (screen point ↔ world ray, `project_line` clips at the camera), `pick(world, ray)` (nearest Part, exact per-shape hit tests in mesh space), `part_box_edges`. Drawing and picking share `draw_list::part_placement`, so clicks hit what's drawn. The view matrix is `glam::camera::rh::view::look_at_mat4`, the same as `render` (glam 0.34 has no `Mat4::look_at_rh`).
- Orientation uses `EulerRot::YXZ` with (y, x, z) argument order, the same as `physics`.
- Zoom never goes closer than `MIN_FOCUS_DISTANCE` to the focus point, so it can't flip past it.
