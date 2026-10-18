# render

Draws a `DrawList` with wgpu. Must never depend on `core`, Instances, scripts or winit.

- `draw_list.rs`: the public input types. Colors are sRGB; the renderer converts to linear. Y up, right-handed. Shapes are 1×1×1 at the origin, so the transform's scale is the size.
- `renderer.rs`: `Renderer::new(target, w, h)` takes anything that converts to `wgpu::SurfaceTarget`. `plan_draws` orders items (opaque grouped by shape, then transparent sorted far to near) into instanced batches. Two pipelines, same shader: the transparent one blends and doesn't write depth. `InstanceRaw` and `FrameUniform` must match `shader.wgsl` exactly.
- `shader.wgsl`: GGX PBR with one sun plus ambient (stage 1).
- `mesh.rs`: block, sphere, cylinder (along X) and wedge. Triangles are counter-clockwise from outside; back faces are culled. Tests check winding, normals and bounds for every mesh, so add new meshes to `all_meshes()`.
- glam 0.34: projections live in `glam::camera::rh` (use `proj::directx`, since wgpu's depth is 0 to 1). The old `Mat4::perspective_rh` is gone.
- Minimized windows are 0×0. Never configure a surface or depth texture with zero size.
