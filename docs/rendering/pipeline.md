# Render pipeline

What the renderer draws, materials, built-in shapes, and which platforms it runs on.

`render` takes a **draw list** (meshes, materials, transforms, lights, camera, UI draw data) and draws it with `wgpu`. It has no knowledge of Instances, scripts, physics or the editor. The editor viewport and the player both feed it the same way.

Draw list conventions: Y up, right-handed, and colors as sRGB from 0 to 1 (the renderer converts them to linear). Built-in shapes are 1×1×1 at the origin, so a part's transform is translation × rotation × scale(Size). The Instance-to-draw-list step lives in the `scene` crate (`build_draw_list`), shared by the player and the editor. UI entries are pixel-positioned rectangles, images and shaped text; the renderer draws them over the 3D scene, in the list's order. It batches them into layers (`ui_pass::ui_layers`): each layer draws its rectangles, then its images, then its text (one glyphon `TextRenderer` per layer with text, kept in a pool), and a new layer starts only where an item would otherwise go under an earlier image or text it overlaps (text boxes count their outline and shadow reach). So a Frame over a TextLabel covers it, and a loading screen at the end of the list covers all of the game's UI, while UI that doesn't overlap stays one layer.

- `Renderer` draws into a window surface it creates itself (the player).
- `TextureRenderer` draws into a texture on a GPU device someone else created (the editor, where eframe owns the device and egui shows the texture). Both share the private `ScenePipeline`, so they draw identically.
- **Frame cost** (v2 Phase 5, for the editor's Profiler): `TextureRenderer::stats` gives the last frame's draw calls and triangles (shadow cascades and the ambient occlusion prepass included), a recent GPU time, and the bytes of textures and meshes on the GPU (graphics Phase 3; see `docs/rendering/shadows.md`). GPU time needs `Features::TIMESTAMP_QUERY`, which only the editor asks for, where the adapter has it (`main.rs`); the player never does, so it never times anything. `renderer/gpu_timer.rs` writes a timestamp with an empty compute pass before and after the frame, resolves them into a buffer, and reads it with `map_async` once the GPU is done, a frame or two later; a frame isn't timed while the previous reading is still being read, so the CPU never waits for the GPU.
- The texture is `Rgba8Unorm` holding sRGB-encoded colors, because that's what egui 0.32 expects of native textures. Drawing goes through an `Rgba8UnormSrgb` view so the GPU encodes the shader's linear output. Sampling an sRGB texture would hand egui linear values and everything would look far too dark.
- **Known issue (found in Phase 8):** on at least one driver (AMD, Vulkan, Windows), the background clear through an sRGB view is *not* sRGB-encoded while shaded pixels are. The player's window surface behaves the same way, so on that machine the sky shows as `(90, 154, 227)` instead of the intended `(161, 204, 242)`. Other drivers may encode it, so the sky color can differ between machines. Fix when the sky becomes a drawn pass (stage 4), or sooner by drawing the background with a full-screen triangle instead of a clear.

Every material carries: base color, metalness, roughness, emissive, opacity. Presets like plastic, metal, glass and neon are just preset values for these fields. Don't add stage-specific material fields that a later stage would have to undo.

How Part properties map onto these (decided in Phase 3):

- `Color`, `Metalness` and `Roughness` are Part properties and feed the draw list directly.
- `Material` (Plastic, Metal, Glass, Neon) is a preset. Choosing it sets `Metalness` and `Roughness` to the preset's values, which you can then adjust (for example, a rougher Metal). See `Material::preset()` in `core`. A part's `MaterialVariant` (a material file) replaces the preset; see `docs/rendering/models.md`.
- Glass is drawn see-through, and Neon gives off its `Color` as emissive light instead of being shaded.
- `Transparency` (Phase 35, Parts and MeshParts, 0 to 1) multiplies the preset's (or the model's) opacity by `1 - Transparency`; a fully transparent part isn't drawn at all, but can still be picked in the editor.
- `EmissiveColor` and `Emission` (Phase 35, Parts) add `EmissiveColor × Emission` to the emissive light on top of any Neon glow, so any material can glow in any color; above 1 it blooms.
- Metals reflect the sky through environment lighting (stage 4).

Transparency: opaque items are drawn first. Then see-through items are drawn, farthest to nearest by their centers, with blending on and depth writes off. Overlapping see-through items can still blend in the wrong order; that's accepted for now.

Built-in primitives are block, sphere, cylinder and wedge, plus imported glTF and FBX models (MeshParts, Phase 17; see Imported models). Part shape rules match Roblox: a Ball's diameter is the smallest of its `Size` axes, a Cylinder lies along X (`Size.X` is its length, its diameter is the smaller of Y and Z), and a Wedge's slope rises from the bottom front (-Z) edge to the top back (+Z) edge. `Orientation` is degrees, applied around Z, then X, then Y.

1. PBR shading with a sun and ambient light (implemented)
2. Sun shadows (implemented in Phase 30)
3. Many point and spot lights (implemented in Phase 31)
4. Sky and environment lighting (implemented in Phase 32)
5. Post-processing: HDR, tone mapping, bloom (implemented in Phase 33)
6. Sprites and particles (Phase 37)

Each stage must leave existing scenes rendering correctly. `examples/hello-graphics` is the graphics reference scene: plastic, metal, glass and Neon parts, a textured PBR MeshPart, a SurfaceAppearance block with `TileSize`, a stretched wedge, a point and a spot light in lamp parts, sun shadows in all three cascades and an emissive part that blooms. Its `reference.png` is the whole 480×270 frame, and the player's `every_example_renders` test fails if the frame drifts from it by 6 or more per channel on average (an example's `reference.png` may also be just its top-left corner, as hello-css's is). After a deliberate change to the look, save the new frame by running that test with `AXCENITY_SNAPSHOTS` set to a folder, check `example-hello-graphics.png` by eye, and copy it over the reference. The render tests also check that draw-list colors survive the sRGB→linear→sRGB round trip, and that a stretched block shades like a unit one.

`wgpu` backends: DirectX 12 or Vulkan on Windows, Metal on macOS, Vulkan on Linux. Avoid features that aren't available on all three.
