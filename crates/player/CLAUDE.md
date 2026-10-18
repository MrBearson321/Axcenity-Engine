# player

Binary that ships with exported games. Must never depend on `editor`, `egui` or `blocks`.

- `main.rs`: winit 0.30 `ApplicationHandler`. The window is created in `resumed`, not before the loop starts. Errors in the loop go through `App::fail` and are returned from `main`.
- `gpu.rs`: temporary wgpu setup plus clear-to-sky. Moves into the `render` crate in Phase 2.
- wgpu is pinned to 25.x. Its API changes between majors (for example, `request_adapter`/`request_device` signatures and render pass attachment fields), so check the changelog when upgrading.
- Minimized windows are 0×0. Never configure a surface with zero size.
