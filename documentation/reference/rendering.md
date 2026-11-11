---
description: How Axcenity draws a frame, materials and shaders, lights and shadows, post-processing, graphics quality presets, levels of detail and what each setting costs.
icon: image
samples: [hello-graphics, hello-pbr, hello-lights, hello-shader, hello-material-graph]
---

# Rendering and graphics

Axcenity draws with **wgpu**: DirectX 12 on Windows, Vulkan on Linux and Metal on macOS. The editor's Scene view and the player use the same renderer, so what you see while building is what players see.

## A frame

The renderer is given a list of things to draw (it never sees Instances or scripts) and draws it in a fixed order:

1. **Shadows** for the sun (in cascades), spot lights and point lights.
2. A **depth prepass**, then ambient occlusion and contact shadows.
3. **Volumetric fog** and GPU particles.
4. The **scene**: solid surfaces, the sky, terrain, water, see-through surfaces, outlines and decals. Materials are physically based (colour, metalness, roughness, normal maps, glow).
5. **Motion vectors** for temporal anti-aliasing and motion blur, then depth of field.
6. **Post-processing**: bloom, tone mapping, colour grading, lens effects and FXAA.
7. **UI** on top.

Lights are sorted into screen tiles, so many small lights cost little. Far objects use simpler copies (**levels of detail**) made at import, and objects outside the view or hidden behind others aren't drawn.

## Look and cost

What a world **looks like** is saved with the world: Lighting's tone mapping, exposure, bloom, sky, fog and shadows, and the lights and effects in it.

What it **costs** belongs to each computer: the **graphics quality** preset (Low, Medium, High, Ultra, or Automatic), set in **Settings > Graphics** in the editor and in the player's own settings. High is the look worlds are made with. Each value can be overridden on its own:

| Setting | Low | Medium | High | Ultra |
|---|---|---|---|---|
| Shadow maps | sun only | + 2 spot, 1 point | + 4 spot, 2 point | + 8 spot, 4 point |
| Shadow distance | 75 studs | 100 | 150 | 300 |
| Post-processing | off | on | on | on |
| Render scale | 75% | 100% | 100% | 100% |
| Texture quality | quarter | half | full | full |
| Anti-aliasing | off | FXAA | MSAA 4× | MSAA 4× |
| Ambient occlusion | off | low | high | high |
| Volumetric fog | off | off | off | on |

TAA (temporal anti-aliasing) and **real-time global illumination** are overrides only: they're off in every preset. Games can change these settings from a LocalScript through **UserGameSettings**, such as `UserGameSettings.GraphicsQuality`.

## Materials and shaders

- A part's **Material** picks a preset look (Plastic, Metal, Wood, Neon…); **Color**, **Metalness** and **Roughness** adjust it.
- A **SurfaceAppearance** gives a part or MeshPart texture maps: colour, normal, metalness, roughness, glow and ambient occlusion.
- **Material files** (`.material.json`), edited in the **Material** tab, save a look to reuse. Their **Graph** view builds a material from nodes.
- Advanced: a material can carry a **surface shader** written in WGSL, the GPU language. The editor checks it as you type. See the **hello-shader** sample. This is for experienced users and is checked, not sandboxed like Lua.

## Measuring

- **View > Panels > Profiler**, **Graphics** tab: draw calls, triangles, shadow views, lights and GPU time.
- **View > View Mode** shows the scene unlit, as wireframe, normals, metalness, roughness, ambient occlusion, levels of detail or overdraw.
- The `fps` command in the Developer Console (F9) says whether a frame waits on the CPU or the GPU.

> **Known limits.** Overlapping see-through objects can blend in the wrong order (they're sorted by their centres). Rect lights are lit from a representative point. glTF vertex colours and emissive strength aren't read.

More: [Rendering in the engineering wiki](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Engine-Systems/Rendering.md).
