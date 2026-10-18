# Rendering

Read before working in the `render` crate or changing material and light properties.

## Boundary

`render` takes a **draw list** (meshes, materials, transforms, lights, camera, UI draw data) and draws it with `wgpu`. It has no knowledge of Instances, scripts, physics or the editor. The editor viewport and the player both feed it the same way.

## Materials (PBR from day one)

Every material carries: base color, metalness, roughness, emissive, opacity. Presets like plastic, metal, glass and neon are just preset values for these fields. Don't add stage-specific material fields that a later stage would have to undo.

## Geometry

Built-in primitives (block, sphere, cylinder, wedge) plus imported **glTF** meshes.

## Stages (build in order; editor and scripting work starts after stage 1)

1. PBR shading with a sun and ambient light ← current target
2. Sun shadows (shadow maps)
3. Many point and spot lights
4. Sky and environment lighting (image-based lighting)
5. Post-processing: HDR, tone mapping, bloom

Each stage must leave existing scenes rendering correctly. Update "current target" above when a stage is done.

## Platform

`wgpu` backends: DirectX 12 or Vulkan on Windows, Metal on macOS, Vulkan on Linux. Avoid features that aren't available on all three.
