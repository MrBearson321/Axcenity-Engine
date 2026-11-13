# Rendering

Map only. `render` draws a draw list. It does not know about Instances. Open the one file that matches the change.

- Pipeline, materials, built-in shapes, platforms: [rendering/pipeline.md](rendering/pipeline.md)
- Imported models, texture maps, material files, surface shaders and material graphs: [rendering/models.md](rendering/models.md)
- Cooked textures (BC7 mip levels made ahead in `.axcenity/imported`, how they load and draw): [rendering/textures.md](rendering/textures.md)
- Animation playback: [rendering/animation.md](rendering/animation.md)
- Point and spot lights, and their shadows: [rendering/lights.md](rendering/lights.md)
- Sky, time of day (sun, moon and stars), Atmosphere, sky images, environment maps, reflection probes, light probe volumes, real-time GI and environment lighting: [rendering/sky.md](rendering/sky.md)
- Fog (Lighting's distance fog, an Atmosphere's height fog, volumetric fog and sun shafts): [rendering/fog.md](rendering/fog.md)
- Clouds (a `Clouds` layer, its shadows, the wind, cloud quality): [rendering/clouds.md](rendering/clouds.md)
- Weather (a `Weather`: rain and snow, wet and snow-covered surfaces, easing): [rendering/weather.md](rendering/weather.md)
- Plants swaying in the wind (`DrawItem::sway`, a material's `sway`, `GlobalWind`): [rendering/wind.md](rendering/wind.md)
- Water (a material file's `water`: waves, seeing through it, reflections, depth tint, foam, the scene copy): [rendering/water.md](rendering/water.md)
- Shading models and outlines (a material file's `shading`: Standard, Unlit, Toon bands, ramp, rim and highlight; its `outline`; Lighting's `OutlineEffect`): [rendering/stylized.md](rendering/stylized.md)
- Bloom, tone mapping, color grading (Lighting's `ColorCorrectionEffect`: brightness, contrast, saturation, tint, `.cube` and strip lookup tables, posterize, palettes) and lens effects (depth of field, vignette, film grain, chromatic aberration, sharpening): [rendering/post.md](rendering/post.md)
- Motion vectors, TAA and motion blur (a `MotionBlurEffect`; what's kept between frames, and the render graph decision): [rendering/temporal.md](rendering/temporal.md)
- Graphics volumes (a `GraphicsVolume`: a place's own exposure, fog, Atmosphere, bloom and color correction, blended in around the camera by `BlendDistance` and `Priority`): [rendering/volumes.md](rendering/volumes.md)
- Sprites and particles: [rendering/particles.md](rendering/particles.md)
- Decals (a Part's `Decal` on a face, `ProjectedDecal` boxes, `workspace:SpawnDecal`, decal quality): [rendering/decals.md](rendering/decals.md)
- Hardware ray tracing (ray queries, the ray traced scene, the ray tracing distance, the Ray Hits and Path Traced views, each ray traced effect): [rendering/ray-tracing.md](rendering/ray-tracing.md)
- The machine's graphics settings (presets, overrides, capabilities, `UserGameSettings`, `graphics.json`, the per-preset references): [rendering/settings.md](rendering/settings.md)
- Measuring drawing (`RenderStats`, per-pass GPU time, the benchmarks and their baseline) and the editor's debug views (Viewport > View Mode: wireframe, normals, overdraw and the rest): [rendering/performance.md](rendering/performance.md)
- Sun shadows, and how shadow quality, render scale, anti-aliasing and ambient occlusion draw: [rendering/shadows.md](rendering/shadows.md)

The Animation panel in the editor is [editor/import.md](editor/import.md). The Lua API is [scripting/api-animation.md](scripting/api-animation.md). Clip and state machine files are [project/animation-files.md](project/animation-files.md). Cutscene sequences and the Sequencer tab are [project/sequences.md](project/sequences.md).
