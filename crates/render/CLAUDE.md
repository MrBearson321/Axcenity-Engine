# render

Draws a `DrawList` with wgpu. Must never depend on `core`, Instances, scripts or winit.

- `draw_list.rs`: the public input types. Colors are sRGB; the renderer converts to linear. Y up, right-handed. Shapes are 1×1×1 at the origin, so the transform's scale is the size.
- `renderer.rs`: `Renderer::new(target, w, h)` takes anything that converts to `wgpu::SurfaceTarget`. One instanced draw per `Shape`. `InstanceRaw` and `FrameUniform` must match `shader.wgsl` exactly.
- `shader.wgsl`: GGX PBR with one sun plus ambient (stage 1). Opacity is written to alpha but not blended yet.
- `mesh.rs`: built-in meshes. Triangles are counter-clockwise from outside; back faces are culled, and a test checks the winding.
- glam 0.34: projections live in `glam::camera::rh` (use `proj::directx`, since wgpu's depth is 0 to 1). The old `Mat4::perspective_rh` is gone.
- Minimized windows are 0×0. Never configure a surface or depth texture with zero size.
