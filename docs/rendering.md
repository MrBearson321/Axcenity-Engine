# Rendering

Read before working in the `render` crate or changing material and light properties.

## Boundary

`render` takes a **draw list** (meshes, materials, transforms, lights, camera, UI draw data) and draws it with `wgpu`. It has no knowledge of Instances, scripts, physics or the editor. The editor viewport and the player both feed it the same way.

Draw list conventions: Y up, right-handed, and colors as sRGB from 0 to 1 (the renderer converts them to linear). Built-in shapes are 1×1×1 at the origin, so a part's transform is translation × rotation × scale(Size). The Instance-to-draw-list step currently lives in `player/src/scene.rs`.

## Materials (PBR from day one)

Every material carries: base color, metalness, roughness, emissive, opacity. Presets like plastic, metal, glass and neon are just preset values for these fields. Don't add stage-specific material fields that a later stage would have to undo.

How Part properties map onto these (decided in Phase 3):

- `Color`, `Metalness` and `Roughness` are Part properties and feed the draw list directly.
- `Material` (Plastic, Metal, Glass, Neon) is a preset. Choosing it sets `Metalness` and `Roughness` to the preset's values, which you can then adjust (for example, a rougher Metal). See `Material::preset()` in `core`.
- Opacity and glow aren't properties; they come from the material. Glass is drawn see-through, and Neon gives off its `Color` as emissive light instead of being shaded. If per-part transparency is needed later, add a `Transparency` property that multiplies the preset's opacity.
- Without environment lighting (stage 4), metals reflect only the sun and ambient light, so they look darker than they will later.

Transparency: opaque items are drawn first. Then see-through items are drawn, farthest to nearest by their centers, with blending on and depth writes off. Overlapping see-through items can still blend in the wrong order; that's accepted for now.

## Geometry

Built-in primitives (block, sphere, cylinder, wedge) plus imported **glTF** meshes. Part shape rules match Roblox: a Ball's diameter is the smallest of its `Size` axes, a Cylinder lies along X (`Size.X` is its length, its diameter is the smaller of Y and Z), and a Wedge's slope rises from the bottom front (-Z) edge to the top back (+Z) edge. `Orientation` is degrees, applied around Z, then X, then Y.

## Stages (build in order; editor and scripting work starts after stage 1)

1. PBR shading with a sun and ambient light ← current target
2. Sun shadows (shadow maps)
3. Many point and spot lights
4. Sky and environment lighting (image-based lighting)
5. Post-processing: HDR, tone mapping, bloom

Each stage must leave existing scenes rendering correctly. Update "current target" above when a stage is done.

## Platform

`wgpu` backends: DirectX 12 or Vulkan on Windows, Metal on macOS, Vulkan on Linux. Avoid features that aren't available on all three.
