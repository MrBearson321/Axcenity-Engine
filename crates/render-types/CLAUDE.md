# `render-types`

- The renderer's plain data, split out of `render` (roadmap M8) so `script`, `net` and the dedicated server don't link wgpu: `GraphicsSettings` and its quality enums, `Capabilities`, `EffectiveGraphics` (`graphics.rs`, behavior in `docs/rendering/settings.md`) and `Camera` (`camera.rs`).
- `render` re-exports everything here; GPU-side code (`render::read_capabilities`) stays in `render`. Never add `wgpu` or any GPU, window or audio dependency.
