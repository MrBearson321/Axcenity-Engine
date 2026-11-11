---
description: Light a world with the sun, sky, lights, shadows, fog, clouds, weather and post effects, and choose graphics quality.
icon: sun
samples: [hello-lights, hello-graphics, hello-grading, hello-lens]
---

# Lighting and rendering

How a world looks is set by the **Lighting** service and the objects in it, plus the lights you place. How much that costs to draw is set separately, per computer, by the graphics settings. You decide the look; each player's machine decides how much of it it can afford.

## The sun, sky and time of day

Select **Lighting** in the Explorer:

- **ClockTime** (0 to 24 hours) and **GeographicLatitude** place the sun and moon.
- Shadows have a softness setting, and the sky can be a colour gradient or an image (an HDR `.exr` or `.hdr` panorama looks best).
- **Atmosphere** (World tab, Lights & Sky) replaces the gradient with a physically based sky that changes colour with the sun, with Density, Offset, Glare and Haze.
- **Clouds**, **Weather** (rain or snow, with wetness and snow cover) and **Fog** add to it. Lighting's Fog section sets a simple distance fog; volumetric fog with sun shafts is a graphics setting.

## Lights

**PointLight**, **SpotLight** and **RectLight** light the area around them. Put one in a part to make a lamp, or insert one with nothing selected and it stands on its own, shown as an icon you can move and turn. Set **Color**, **Brightness** and **Range** (and a spot's **Angle**, a rect's **Width** and **Height**); turn on **Shadows** to have it cast them. The **Lighting** tool (**U**) adds and tunes lights in many parts at once. **Viewport > Light Cones** shows where every light reaches. A part's **CastShadow** stops it casting a shadow; **Material** Neon and a part's **Emission** make it glow.

## Reflections and indirect light

- A **Reflection Probe** captures its surroundings for shiny surfaces nearby, and **Light Probes** capture soft bounced light over an area. **Bake Probes** (World tab) bakes them all; a probe whose bake is out of date shows a warning in the Explorer, so bake again after changing the scene.
- **Real-time global illumination** is a graphics setting, off at every preset because it is costly.

## Post-processing and effects

**Bloom**, **Exposure**, **Color Correction** (brightness, contrast, saturation, tint, colour lookup tables), **Outlines**, and lens effects (**Depth of Field**, **Vignette**, **Film Grain**, **Aberration**, **Sharpen**, **Motion Blur**) go in Lighting. A **Graphics Volume** is an invisible box that blends its own effects, fog and atmosphere in as the camera enters it, for a dark cave or an underwater look.

## Particles

A **ParticleEmitter** in a part gives off particles (fire, smoke, sparks). The **Emission** tool (**P**) adds emitters set up as common effects and shows a burst. Emitters show while you edit, not only during Play. The wind (Workspace's **GlobalWind**) can blow them.

## Graphics quality

**Settings > Graphics** sets this computer's preset (**Low**, **Medium**, **High** or **Ultra**) and overrides for shadows, shadow distance, post-processing, render scale, texture quality and anti-aliasing (FXAA, MSAA or TAA). The panel says what is in use and why anything was lowered. Below 100% render scale the scene is drawn smaller and then upscaled with an edge-aware filter and sharpened (in the style of AMD FSR 1), which looks noticeably crisper than a plain stretch; there's nothing to switch on. **Debug > Graphics Benchmark…** measures this computer and recommends the editor's settings. In a built game players choose the same from the game's settings. These settings never change your project.

## Seeing how it's drawn

**Viewport > View Mode** switches the Scene to debug views such as wireframe, normals and overdraw. **Viewport > Overlays > Culling** shows what is drawn and what is skipped. The **Profiler** panel's Graphics tab counts draw calls, triangles and video memory, and times each rendering pass on the GPU. See [Debugging and performance](debugging-and-performance.md).

> **Known limits.** Overlapping see-through objects can blend in the wrong order. Lights are sorted in screen tiles, so very many lights at different depths cost more than they would with clustering. Rect lights light from one representative point.
