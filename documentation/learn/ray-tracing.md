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
| **Ray tracing: kept off by AXCENITY_RAY_TRACING=0** | The editor was started from a window where that variable was set, which keeps ray tracing off. Click **Restart now**, or open a new terminal window to start the editor from. |
| **Ray tracing: not on this GPU** | This card or its driver can't ray trace. Everything is drawn the usual way. |

Click **Compare** on the badge to see the view without the ray traced effects, and again to bring them back: the best way to see what they add. Hover the badge for more detail. **Viewport > View Mode > Ray Hits** shows what the rays hit, each object in its own colour.

**Viewport > View Mode > Path Traced** shows a reference picture: light followed ray by ray as it bounces around the scene, the way offline renderers work. It starts grainy and clears up the longer the camera stays still (the legend counts the paths so far); moving the camera or anything in the scene starts it again. Use it to see how your lighting should look and compare the normal view against it. It treats every surface as matte and leaves out glass, water and fog.

## Get good shadows

- Shadows come from the **sun**, so set **Lighting > ClockTime** to a time when it's up. Low morning or evening sun gives long shadows.
- **Lighting > ShadowSoftness** decides how soft they are, from sharp (0) to soft. Ray traced shadows get softer the farther a shadow falls from what casts it, as real ones do. **Low**, **Medium** and **High** only change how many rays smooth that softness.
- Rays reach as far as **Ray tracing distance** (Settings > Graphics). Past it, the usual shadow maps take over, so a long distance costs more but traces more of the world.

## Get good lighting

- **Ray traced ambient occlusion** darkens corners, gaps and the ground under objects. Lighting's **AmbientOcclusion** still sets how strong that is.
- **The sky.** A sky picture (**Lighting > SkyImage**, such as the default sky) looks real but keeps its own colours. Changing ClockTime moves the sun and darkens the picture toward night, but sunsets stay the picture's. For a sky that turns orange at sunset and dark at night, click **Follow time of day** under the SkyImage row. It adds an **Atmosphere**.
- **Bounced light.** Turn on **Ray traced GI** (Settings > Graphics > Ray tracing). Rays light a grid of light probes around the camera every frame, so a red wall throws red light, a room darkens when its door closes and light follows the sun as it goes down. Nothing needs placing or baking. Light Probes parts you've baked are kept up to date too. Reflection Probes still give reflections; bake them after big changes.

## Light a room

Light gets into a room through its windows and doors.

- **Let the sun in.** A low sun (**Lighting > ClockTime** around 8 or 17) shines through the windows onto the floor. Thick clouds (**Clouds > Cover** near 1) hide the sun, so the room gets only soft sky light.
- **Turn on Ray traced GI.** The sunny patch on the floor then lights the rest of the room, warm off a wooden floor, and corners away from the windows stay darker. Without it, a room gets the sky's light as if it had no walls, so it looks flat and bluish.
- **Light shafts.** Beams in the air need something in the air to show in. Give **Lighting** an **Atmosphere** and raise its **Density** and **Haze**. **Volumetric fog** (Settings > Graphics, on at Ultra) draws the beams.
- **Without a ray tracing card**, put a **Light Probes** part around the room's inside with a small **Spacing** (2 to 3 studs), and click **Bake Probes**.

## What it costs

Each effect is timed in the **Profiler** panel's Graphics tab. **Low** effects are cheapest. Lower an effect, or the Ray tracing distance, if the frame rate drops. Without a ray tracing card nothing is spent on any of it.

## In your game

Players choose the same in the game's own graphics settings. A LocalScript can set them through `UserGameSettings`: `RayTracedShadows`, `RayTracedAmbientOcclusion` and `RayTracedGlobalIllumination`. Every effect is off unless a player turns it on, because not every card can ray trace.

## What to look for

Ray traced shadows and ambient occlusion are about accuracy, so in a sunny open scene they look close to good shadow maps. The difference shows up close:

- **Where things touch.** The ground right under a car, a porch step or a crate gets a tight, dark contact shadow instead of a gap or a blur.
- **Soft edges that follow distance.** A post's shadow is sharp at its foot and softens toward its tip.
- **Corners and gaps.** Under eaves, between boards and beneath furniture, ambient occlusion comes from what's really there, even just off screen.
- **Thin things.** Fence rails, wires and poles keep their shadows, which shadow maps can lose far from the camera.

Bounced light that changes with the world (Ray traced GI) makes the biggest visible difference, especially indoors and in shade.
