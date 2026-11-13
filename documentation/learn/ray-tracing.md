---
description: Turn on hardware ray tracing in a couple of clicks, check that it's working, and get good ray traced shadows and lighting.
icon: sun
samples: [hello-graphics, hello-lights]
---

# Ray tracing: the best look, simply

Ray tracing follows real rays of light through the world. It gives exact shadows up close and soft contact darkening in corners and under objects, even for things just off screen. It's optional, it needs a graphics card that can ray trace, and everything still looks right without it.

## Turn it on

1. Open **Settings > Graphics** and scroll to **Ray tracing**.
2. Click **Use the ray traced look**. This one click turns on Hardware ray tracing, ray traced shadows and ambient occlusion at Medium, and what they look best with: shadows and reflections at High or above, and TAA, which smooths the rays. Your other settings stay as they are.
3. Click **Restart now**, there or on the Scene view's badge. The graphics card is set up for ray tracing when the editor starts, so it's needed the first time. Your project opens again as it was, unsaved changes to the world and scripts included (save Event Graphs, behavior trees, data tables and materials first). If it still asks for a restart, an old Desktop shortcut may be opening an older installed copy that can't ray trace: start this version instead.

**Ray traced effects off** turns the effects off again without a restart.

## Check that it's working

While Hardware ray tracing is on, a small badge sits at the bottom right of the Scene view:

| The badge says | What it means |
|---|---|
| **Ray tracing: Shadows Medium, AO Medium · 812 items traced** | It's working: these effects are drawn, and that many objects are in the ray traced scene. |
| **Ray tracing on: no effects** | The card is ready, but no ray traced effect is on. Click **Use the ray traced look**. |
| **Ray tracing: restart to turn on** | Click **Restart now** beside it. |
| **Ray tracing: not on this GPU** | This card or its driver can't ray trace. Everything is drawn the usual way. |

Hover the badge for more detail. **Viewport > View Mode > Ray Hits** shows what the rays hit, each object in its own colour.

## Get good shadows

- Shadows come from the **sun**, so set **Lighting > ClockTime** to a time when it's up. Low morning or evening sun gives long shadows.
- **Lighting > ShadowSoftness** decides how soft they are, from sharp (0) to soft. Ray traced shadows get softer the farther a shadow falls from what casts it, as real ones do. **Low**, **Medium** and **High** only change how many rays smooth that softness.
- Rays reach as far as **Ray tracing distance** (Settings > Graphics). Past it, the usual shadow maps take over, so a long distance costs more but traces more of the world.

## Get good lighting

- **Ray traced ambient occlusion** darkens corners, gaps and the ground under objects. Lighting's **AmbientOcclusion** still sets how strong that is.
- **The sky.** A sky picture (**Lighting > SkyImage**, such as the default sky) looks real but keeps its own colours. Changing ClockTime moves the sun and darkens the picture toward night, but sunsets stay the picture's. For a sky that turns orange at sunset and dark at night, click **Follow time of day** under the SkyImage row. It adds an **Atmosphere**.
- **Light Probes** and **Reflection Probes** still give bounced light and reflections. Bake them after big changes with **Bake Probes** on the World tab.

## What it costs

Each effect is timed in the **Profiler** panel's Graphics tab. **Low** effects are cheapest. Lower an effect, or the Ray tracing distance, if the frame rate drops. Without a ray tracing card nothing is spent on any of it.

## In your game

Players choose the same in the game's own graphics settings. A LocalScript can set them through `UserGameSettings`: `RayTracedShadows` and `RayTracedAmbientOcclusion`. Every effect is off unless a player turns it on, because not every card can ray trace.
