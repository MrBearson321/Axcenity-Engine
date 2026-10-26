# `render`

- Draws 3D and screen-space UI `DrawList` data with wgpu. Pipeline and materials: `docs/rendering/pipeline.md`. Other drawing topics are listed in `docs/rendering.md`; open one.
- `renderer.rs` is `Renderer` and `TextureRenderer`; `renderer/types.rs` has the GPU types and `renderer/scene.rs` draws a frame. `lighting.rs` plans uniforms, tiles, shadows and batches. `post.rs` blooms. `ui_pass.rs` builds quads. `gpu.rs` uploads resources. `sprites.rs` draws particles. `fonts.rs` lists the built-in UI fonts (`BUILT_IN_FONTS`, plus bold and italic `VARIANTS`); a project's fonts come in `DrawList::fonts`. `renderer/text.rs` picks faces, fits `TextScaled` text and makes outline, shadow, faux bold and decoration passes.
- Do not add dependencies on `core`, Instances, scripts or winit; world-to-draw conversion belongs in `scene`. Posed meshes keep their key and change `MeshSource::version`; `update_custom_mesh` rewrites their corners.
- Keep `SpriteRaw` and `SpriteUniform` (`renderer/sprites.rs`) aligned with `sprite.wgsl`.
- Keep `InstanceRaw`, `FrameUniform`, `MapsUniform`, the vertex layout and material bindings aligned with `shader.wgsl`, and coordinate wgpu API changes with the workspace version.
