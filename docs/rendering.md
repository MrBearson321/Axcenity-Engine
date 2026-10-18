# Rendering

Read before working in the `render` crate or changing material and light properties.

## Boundary

`render` takes a **draw list** (meshes, materials, transforms, lights, camera, UI draw data) and draws it with `wgpu`. It has no knowledge of Instances, scripts, physics or the editor. The editor viewport and the player both feed it the same way.

Draw list conventions: Y up, right-handed, and colors as sRGB from 0 to 1 (the renderer converts them to linear). Built-in shapes are 1×1×1 at the origin, so a part's transform is translation × rotation × scale(Size). The Instance-to-draw-list step lives in the `scene` crate (`build_draw_list`), shared by the player and the editor.

## Targets (decided in Phase 8)

- `Renderer` draws into a window surface it creates itself (the player).
- `TextureRenderer` draws into a texture on a GPU device someone else created (the editor, where eframe owns the device and egui shows the texture). Both share the private `ScenePipeline`, so they draw identically.
- The texture is `Rgba8Unorm` holding sRGB-encoded colors, because that's what egui 0.32 expects of native textures. Drawing goes through an `Rgba8UnormSrgb` view so the GPU encodes the shader's linear output. Sampling an sRGB texture would hand egui linear values and everything would look far too dark.
- **Known issue (found in Phase 8):** on at least one driver (AMD, Vulkan, Windows), the background clear through an sRGB view is *not* sRGB-encoded while shaded pixels are. The player's window surface behaves the same way, so on that machine the sky shows as `(90, 154, 227)` instead of the intended `(161, 204, 242)`. Other drivers may encode it, so the sky color can differ between machines. Fix when the sky becomes a drawn pass (stage 4), or sooner by drawing the background with a full-screen triangle instead of a clear.

## Materials (PBR from day one)

Every material carries: base color, metalness, roughness, emissive, opacity. Presets like plastic, metal, glass and neon are just preset values for these fields. Don't add stage-specific material fields that a later stage would have to undo.

How Part properties map onto these (decided in Phase 3):

- `Color`, `Metalness` and `Roughness` are Part properties and feed the draw list directly.
- `Material` (Plastic, Metal, Glass, Neon) is a preset. Choosing it sets `Metalness` and `Roughness` to the preset's values, which you can then adjust (for example, a rougher Metal). See `Material::preset()` in `core`.
- Opacity and glow aren't properties; they come from the material. Glass is drawn see-through, and Neon gives off its `Color` as emissive light instead of being shaded. If per-part transparency is needed later, add a `Transparency` property that multiplies the preset's opacity.
- Without environment lighting (stage 4), metals reflect only the sun and ambient light, so they look darker than they will later.

Transparency: opaque items are drawn first. Then see-through items are drawn, farthest to nearest by their centers, with blending on and depth writes off. Overlapping see-through items can still blend in the wrong order; that's accepted for now.

## Geometry

Built-in primitives are block, sphere, cylinder and wedge, plus imported glTF and FBX models (MeshParts, Phase 17; see Imported models). Part shape rules match Roblox: a Ball's diameter is the smallest of its `Size` axes, a Cylinder lies along X (`Size.X` is its length, its diameter is the smaller of Y and Z), and a Wedge's slope rises from the bottom front (-Z) edge to the top back (+Z) edge. `Orientation` is degrees, applied around Z, then X, then Y.

## Imported models (decided in Phase 17)

- **Draw list:** `Shape::Mesh(Arc<dyn MeshSource>)` is a mesh the caller owns (positions, normals, optional texture coordinates, `u32` indices), and `Material::base_color_texture` an `Arc<dyn TextureSource>` (sRGB RGBA8). They're traits so `scene` hands over the loaded model's data without copying it each frame. The renderer uploads each on first sight, keyed by `key()`, and frees what hasn't been drawn for 600 frames. Broken data (an index past the last corner, the wrong number of pixels) is logged once and not drawn.
- **Vertices** gained texture coordinates (location 12; the built-ins' are all 0). Textures are bound as group 1 with a repeating, mipmapped linear sampler; items without a texture (all built-in shapes) bind a 1×1 white texture. Mipmaps are made on the CPU at upload by 2×2 averaging (in sRGB, slightly off but not visibly). Levels bigger than the GPU allows are left out, so huge textures show less sharp instead of failing.
- **Model space:** `assets` fits every model into the 1×1×1 cube centered on the origin, like the built-ins, so `Size` stretches its box. It walks the default scene's node tree (or every mesh, if there's no scene), applying node transforms (mirrored nodes get their triangles flipped back); missing normals are made smooth from the triangles. Only triangle lists are drawn; points, lines, strips and fans are skipped.
- **FBX** files are read by `ufbx`, which converts them to glTF's space (Y up, right-handed, meters) whatever axes and units they were saved in, and triangulates polygons. Every visible node with a mesh is drawn, at its world transform (pivots and geometry offsets included), one draw item per material it uses. Normals the file lacks are made by `ufbx`. Materials become metallic-roughness: `ufbx` maps Lambert/Phong and other FBX shading models (and Blender's Principled BSDF) to PBR values; base color × factor, opacity (below 1 is `BLEND`; an opacity map makes it `MASK` at 0.5), metalness (default 0), roughness (default 1, Phong shininess converted), emission × factor, and the base color texture with V flipped to the engine's top-left origin. Not yet: skinning (skinned meshes show in their bind pose), animation, other texture maps, and per-UV-set texture choice (the first UV set is used).
- **Materials:** each glTF primitive is one draw item with its metallic-roughness material: base color factor × base color texture, metalness, roughness, emissive (converted from glTF's linear light to the draw list's sRGB). Alpha mode `OPAQUE` is solid, `MASK` cuts out pixels below its cutoff (`Material::alpha_cutoff`, a `discard` in the shader), `BLEND` is drawn see-through (opacity capped just below 1 so it always takes the see-through path). Solid items always write alpha 1. Not yet: metallic-roughness, normal, occlusion and emissive textures, texture transforms, vertex colors, `KHR_materials_emissive_strength`, double-sided materials (back faces are still culled), skins and animation.

## Stages

1. PBR shading with a sun and ambient light (implemented)
2. Sun shadows (shadow maps; next)
3. Many point and spot lights
4. Sky and environment lighting (image-based lighting)
5. Post-processing: HDR, tone mapping, bloom

Each stage must leave existing scenes rendering correctly.

## Platform

`wgpu` backends: DirectX 12 or Vulkan on Windows, Metal on macOS, Vulkan on Linux. Avoid features that aren't available on all three.
