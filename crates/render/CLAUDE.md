# `render`

- Draws 3D and screen-space UI `DrawList` data with wgpu; see `docs/rendering.md` for color, material, geometry and platform contracts.
- `renderer.rs` owns GPU types and draws a frame. `lighting.rs` plans uniforms, tiles, shadows and batches. `post.rs` blooms. `ui_pass.rs` builds quads. `gpu.rs` uploads resources. `sprites.rs` draws particles.
- Do not add dependencies on `core`, Instances, scripts or winit; world-to-draw conversion belongs in `scene`.
- Keep `SpriteRaw` and `SpriteUniform` (`renderer/sprites.rs`) aligned with `sprite.wgsl`.
- Keep `InstanceRaw`, `FrameUniform`, `MapsUniform`, the vertex layout and material bindings aligned with `shader.wgsl`, and coordinate wgpu API changes with the workspace version.
