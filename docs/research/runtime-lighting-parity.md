# Runtime lighting parity (2026-11-14)

Paul's Priority 5 of 2026-11-14: the lighting work of the last days (RT0–RT3, the lighting audit's LE1–LE7) is no longer a basic or missing subsystem, but the Path Traced view is an editor reference mode, not a runtime feature, and the hardware ray tracing plan's runtime stages should be tracked apart from what's merged. The other gap is dynamic global illumination on a wider range of hardware. The ask is selective, measurable improvements, not a Lumen equivalent. This document audits what a **played game** (the player, a Build Game export) gets today against what only the editor gets, and orders the work. The earlier plans are [hardware-ray-tracing.md](hardware-ray-tracing.md) (RT0–RT5) and [lighting-engine-audit.md](lighting-engine-audit.md) (LE0–LE7); what's built is [../rendering/ray-tracing.md](../rendering/ray-tracing.md).

## What a played game gets

The player and the editor draw through the same `render` crate, so every effect below runs in a game once its setting is on. The difference is how a setting gets turned on, and on which GPUs.

| Feature | In the editor | In a played game | Gap |
|---|---|---|---|
| Hardware ray tracing (the device opened with ray queries, RT0) | Settings > Graphics > Ray tracing, Restart now, the viewport badge | Only if the player's own `graphics.json` has `"ray_tracing": true` (or `AXCENITY_RAY_TRACING=1`). A script can't turn it on, can't ask whether the GPU could, and the game's maker can't make it the default | **Missing: RL2** |
| RT sun shadows (RT1), RT lamp shadows (LE1), RT fog shafts (LE3) | Settings, one-click look | `UserGameSettings.RayTracedShadows`, once the device has ray queries | Works, but unreachable without the above |
| RT ambient occlusion (RT3) | Settings | `UserGameSettings.RayTracedAmbientOcclusion` | As above |
| RT probe GI with camera cascades (RT2, #338) | Settings | `UserGameSettings.RayTracedGlobalIllumination` | As above; and nothing at all without RT hardware (RL3) |
| RT water reflections (LE5) | Reflection quality High | Reflection quality High | As above |
| Characters (skinned meshes) in ray traced effects | Left out of the ray traced scene | Left out | **Missing: RL1.** With RT shadows on, a player's character casts **no shadow** within the ray tracing distance (the shadow maps only cover past it), and its feet get no RT AO. A game shows this more than the editor, since games are about characters |
| Swaying plants in RT (#338) | Yes | Yes | None |
| Path Traced view (LE6) | Viewport > View Mode | No (debug views are an editor-only feature) | **By design**: a reference picture to compare the lit view against, not a runtime renderer. Not planned for games |
| Ray Hits view (RT0) | Yes | No | By design (debug view) |
| Viewport badge, Compare | Yes | F9 console shows no ray tracing line | **Small: RL2** adds RT stats to the console's Stats tab |
| Real-time GI (raster probes, Phase 20, R18.4 tiers) | Settings | `UserGameSettings.RealtimeGI`, on any GPU | Works, but costly: six drawn faces a probe, only volume probes (no camera cascades), so open worlds get none |
| Baked probe volumes, sky light kept apart (R18.3) | Bake Probes | Yes | Static: a moving sun's bounce isn't followed without RT2 or real-time GI |

So the runtime ray traced stages are built (RT0–RT3, LE1, LE3, LE5), but a shipped game can't turn them on, and characters are missing from them. Those two come first, because they're cheap and they decide whether any of the rest reaches a player.

## Dynamic GI on GPUs without ray tracing

What a GPU without ray queries (GTX 10-series, RX 5000, most integrated GPUs, Macs before macOS 15) gets today:

- **Baked volumes** with the sky's share apart (R18.3): follow the sky's brightness and tint, not a moving sun, a lamp switched on or a wall moved.
- **Real-time GI** (raster): re-draws six faces of the whole scene per probe, two probes a frame (`RealtimeGiTier`). It's the only dynamic option, it covers only hand-placed volumes, and in a heavy scene it costs whole extra scene draws (434 of 474 ms a frame in the 36,000-item forest before its heavy-scene path, `performance.md`).

Options weighed:

| Option | Covers | Cost | Verdict |
|---|---|---|---|
| **Software-traced probe GI**: RT2's exact shader and camera cascades, its rays traced in a compute shader against a BVH of simplified meshes kept in one storage buffer instead of a hardware TLAS | Every GPU with compute shaders (all of Axcenity's desktop targets); open worlds through the camera cascades; moving sun, lamps, moved parts | A few thousand rays a frame (Low: 128 probes × 64 rays), each walking a two-level BVH; the BVH of each mesh built once on the CPU from its lowest level of detail, the top level rebuilt when items move | **RL3**. The probes' layout, cascades, flags and shading stay RT2's, so one shader serves both; only the trace function differs |
| Real-time GI's raster faces for the camera's cascades | As above | Six scene draws per probe: thousands of probes a second is far beyond any budget | No |
| Screen-space GI | On-screen bounce | 1–2 ms plus a denoiser; misses off-screen light | No (as in R18) |
| Voxel cone tracing or SDFGI | Fully dynamic, sharper | A second scene representation, voxelization each frame | No: the roadmap says not to attempt this, and RL3 reuses everything RT2 built |
| Lumen-style surface cache | Sharp indirect light and reflections | Months of work | No |

The research plan of 2026-11-12 advised against a software tracer while the only goal was ray traced effects ("a second tracing path doubles the work"). That's different now: the goal is dynamic GI on hardware without rays, the probes' tracing is a few thousand rays a frame (not one a pixel), and RT2 already isolated tracing in `trace_through_holes`, so the software path is a different version of that one function, not a second renderer. Shadows, AO and reflections stay hardware-only: at one or more rays a pixel a software BVH costs too much, and the raster fallbacks for those (cascades, SSAO, probes) are good.

## Order of work (one PR each, from `main`)

| ID | Work | Measured by | PR |
|---|---|---|---|
| RL0 | This audit | | #345 |
| RL1 | Posed (skinned) characters in the ray traced scene: each frame a compute pass poses the mesh's corners by the item's joints (as `pose_corner` does) into a copy built into the item's own BLAS, nearest first within `POSED_TRIANGLES` (300,000 a frame), like swaying plants (#338). Characters cast RT shadows and take part in RT AO, GI and reflections | GPU test: a posed blocker shades a wall where its joints put it, the rest pose doesn't; PC: cost per character at 1440p || #346 |
| RL2 | Games can use ray tracing: `UserGameSettings.HardwareRayTracing` (saved, takes effect at the next start), `RayTracingSupported` and `RayTracingActive` (read only), a project setting for the game's default on a GPU that can ray trace (the one-click ray traced look on first start), ray tracing lines in the F9 console's Stats, the Learn guide's game section | Player tests: a first start on an RT GPU with the project default gets the look and saves it; a script's switch is saved and applied at the next start || #354 |
| RL3 | Software-traced probe GI on GPUs without ray queries (above): `RayTracedGlobalIllumination` works on every GPU with compute shaders, hardware rays where there are some | GPU tests on lavapipe without ray queries: the red wall bounces red, a probe inside a block is left out, a mesh is traced with its simplest copy; PC: the forest and Farmhouse at Low/Medium/High, software against hardware (with the shared benchmark) | #364 |
| RL4 | RT4 reflections (hybrid): glossy surfaces under a roughness cutoff (0.15, 0.3, 0.5) trace one mirror ray (Low at half size), hits lit as GI lights them, misses and rough surfaces keep reflection probes (`RayTracedReflections`) | GPU tests on lavapipe: a mirror floor shows a block off the screen, a rough floor is unchanged, Off frees it, no ray queries is off with a note; PC: cost at 1440p (with the shared benchmark) | #381 |
| RL5 | Runtime performance evidence: the player's `--bench` and `shoot_project` with each ray traced effect and software GI on Paul's projects and S10k on the RX 9070 XT; numbers into `docs/rendering/performance.md` (most RT sections still say "numbers from the PC check") | The table itself | |

RT5's other items (GPU-written TLAS instances, a hybrid shadow tile classifier) stay "only if a measurement asks", and RL5 is that measurement. Not planned: a runtime path tracer, ReSTIR, a full Lumen equivalent.

## Sources

- Axcenity: [hardware-ray-tracing.md](hardware-ray-tracing.md), [lighting-engine-audit.md](lighting-engine-audit.md), [global-illumination.md](global-illumination.md), [../rendering/ray-tracing.md](../rendering/ray-tracing.md), [../rendering/sky.md](../rendering/sky.md) (real-time GI), [../rendering/performance.md](../rendering/performance.md); code: `crates/player/src/graphics.rs` and `main.rs` (`want_ray_tracing` from the saved settings only), `crates/script/src/user_settings.rs` (no hardware ray tracing property), `crates/render/src/renderer/ray_tracing.rs` (posed items left out before RL1).
- Unreal Engine 5: Lumen's software ray tracing against distance fields as the fallback below hardware ray tracing (Lumen technical details, linked in hardware-ray-tracing.md).
- Unity: Surface Cache GI's software BVH fallback on GPUs without ray tracing (UnifiedRayTracing, linked in hardware-ray-tracing.md).
