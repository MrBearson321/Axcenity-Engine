# `render`

- Draws `DrawList`s with wgpu; see `docs/rendering.md` for color, material, geometry and platform contracts.
- Do not add dependencies on `core`, Instances, scripts or winit; world-to-draw conversion belongs in `scene`.
- Keep `InstanceRaw`, `FrameUniform`, `MapsUniform`, the vertex layout and material bindings aligned with `shader.wgsl`, and coordinate wgpu API changes with the workspace version.
