# Lighting engine audit (2026-11-13)

Paul's request of 2026-11-13: go through the lighting engine as it is after his own ray tracing changes, compare it with another engine's, decide what's missing, incomplete or unnecessary, and make the changes. Named asks: check that ray tracing is real ray tracing (against NVIDIA's tools), ray traced shadows from lamps, an easy way to make big fog and haze volumes with interiors excluded, water that goes with the terrain toolkit, and a path tracing mode. References: Unreal Engine 5.x (the closest engine with a full hardware ray tracing tier) and NVIDIA's RTX SDKs (RTXDI, RTXGI, NRD, RTX Path Tracing). The build plan before this is [hardware-ray-tracing.md](hardware-ray-tracing.md) (RT0–RT5); what's built is [../rendering/ray-tracing.md](../rendering/ray-tracing.md).

## Is the ray tracing real?

Yes, for every effect named "ray traced". Each traces wgpu ray queries (`rayQueryInitialize` / `rayQueryProceed`, the same inline ray tracing as DXR 1.1's `RayQuery` and Vulkan's `VK_KHR_ray_query`, which NVIDIA's RTXDI and RTXGI samples use) against a hardware acceleration structure built by the driver: a BLAS per mesh from its own triangles and one TLAS of the items in reach (`renderer/ray_tracing.rs`). Nothing is a screen-space stand-in.

| Effect | How it's traced | Shader |
|---|---|---|
| Sun shadows (RT1) | Rays from each prepass pixel toward the sun's disc, cut-outs alpha-tested per candidate hit | `ray_shadows.wgsl` |
| Ambient occlusion (RT3) | Cosine-weighted hemisphere rays, nearest hit | `ray_ao.wgsl` |
| Global illumination (RT2) | DDGI-style: spherical Fibonacci rays per probe, hits lit by the sun and lamps through shadow rays, blended into L1 SH probes (RTXGI's DDGI idea, without its depth moments) | `ray_gi.wgsl` |
| Ray Hits view | One primary ray per pixel | `ray_hits.wgsl` |

Evidence that they're rays, not the old passes: the tests in `renderer/tests/ray_tracing.rs` shade with **no shadow map drawn**, find a wall **just off screen** that screen-space AO can't see, and pass light through a cut-out's alpha holes; the Ray Hits view shows the TLAS directly.

What isn't ray traced, and should be said plainly:

- **Sun Rays** (`SunRaysEffect`, `sun_rays.wgsl`) is a screen-space radial blur from the sun's position over the depth buffer. It's an artistic effect, like Unreal's Light Shafts, and stays one.
- **Volumetric fog's shafts** read the sun's **shadow maps** (`cell_sun_visibility`), not rays, so a shaft is as sharp as the nearest cascade and cut-out leaves block it as solid cards. And a shaft needs air to show in: indoors the world's height fog is usually too thin, which is why shafts through windows took a hand-made dust box (one at a time, the nearest to the camera).
- **Lamps' shadows** are shadow maps only, and only for lamps with `Shadows` on, a few per quality level (Medium: 2 spot, 1 point). That's the "lights don't cast ray traced shadows" Paul saw.
- **Reflections** of solid surfaces are reflection probes and the sky; water marches the screen at High. No reflection is ray traced yet (RT4 was never built).

## Compared with Unreal Engine 5 and NVIDIA RTX

| Area | Unreal 5 / NVIDIA | Axcenity now | Decision |
|---|---|---|---|
| Direct light shadows | RT shadows for directional, point, spot and rect lights, soft by each light's source radius / area (UE); RTXDI / MegaLights sample many lights stochastically | Sun only (RT1); lamps use shadow maps, a few at a time | **Missing → LE1**: ray traced shadows for every lamp in reach, soft by source size, rect lights sampled over their area |
| Many lights | MegaLights, RTXDI (ReSTIR DI) | Tiled forward lighting, 64 lamps a tile, unshadowed past the map budget | **Incomplete**: LE1 shadows the first 8 lamps a pixel sees by rays; ReSTIR stays out (not needed at Axcenity's lamp counts) |
| Diffuse GI | Lumen (surface cache, SW or HW rays); RTXGI DDGI probes | RT2: DDGI-style probes following the camera plus volumes, multi-bounce | **Done**; keep. Probe depth moments (DDGI's leak fix) not needed: inside-wall probes are already rejected (`GI_INSIDE`) |
| Ambient occlusion | RTAO (UE, only without Lumen) | RT3 | **Done** |
| Reflections | Lumen reflections / RT reflections | Probes; water screen-space at High | **Missing → LE5**: water reflections traced by rays (the most visible mirror in most worlds); general RT reflections stay RT4, later |
| Denoising | NRD (ReBLUR, SIGMA for shadows) | Own bilateral blur for sun shadows; TAA | **Enough**: sun's blur kept; lamps' soft shadows lean on TAA and per-frame turned samples |
| Volumetric fog | Exponential Height Fog + froxel Volumetric Fog; **Local Fog Volumes** (boxes / spheres of fog placed in the level) | Height fog (Atmosphere), froxel grid at Ultra, one "dust" box (a `FogEffect` Density in a GraphicsVolume, the nearest one only) | **Incomplete → LE2**: a `FogVolume` part: many at once, sized boxes, priority, soft edges, height falloff, and an **Interior** mode that clears or replaces the outdoor fog inside it |
| Shafts through windows | Volumetric fog sampling RT / virtual shadow maps | Froxels sample the sun's cascades | **Incomplete → LE3**: with ray traced shadows on, froxels trace their sun visibility by rays (glass lets light through, cut-outs' holes too) |
| Water | Water plugin: Water Bodies (ocean, lake, river) integrated with Landscape, single-layer water shading, waves | A water material on a Part; no terrain link | **Missing → LE4**: terrain water: a sea level and painted lakes in the Terrain Studio, drawn only where the ground is below it, with foam at the shore |
| Path tracing | UE Path Tracer view mode (progressive, reference); NVIDIA RTX Path Tracing SDK (RTXPT) | None | **Missing → LE6**: a progressive Path Traced view mode in the editor (and `TextureRenderer` for stills) |
| Area lights | Rect lights with LTC shading and area shadows | Rect lights from a representative point | **Incomplete**: LE1 traces their shadows over the whole rectangle; LTC shading later |
| Skinned meshes in RT | Dynamic BLAS updates | Posed characters left out | **Incomplete, deferred** (RT5): a BLAS rebuild per character per frame costs too much without refit in wgpu |
| Cut-outs in AO and GI | Alpha-tested any-hit everywhere | Only RT shadows test holes | **Incomplete → LE7**: AO and GI test holes the way shadows do (shared `cut_out_covers`) |

### Unnecessary or superseded

- **Dust in a GraphicsVolume** (`FogEffect.Density`): superseded by `FogVolume` (LE2). Kept loading and drawing so saved worlds look the same; the ribbon offers Fog Volume instead.
- **Real-time GI's raster faces** (`realtime_gi.rs`): superseded by RT2 on ray tracing GPUs, kept as the fallback for others. Nothing to remove.
- **Screen-space Sun Rays**: not superseded (it's a look, and cheap), but with LE2 and LE3 a fog volume gives true shafts; the Learn guide says which to use.
- Nothing found that should be deleted outright: every raster pass is the fallback for a GPU without ray queries, which the development policy keeps.

## Order of work (one PR each, from `main`)

| ID | Work | PR |
|---|---|---|
| LE0 | This audit | this PR |
| LE1 | Ray traced lamp shadows (point, spot, rect), soft by size, under the Ray traced shadows setting | |
| LE2 | `FogVolume`: easy local fog and haze, many at once, with Interior (exclusion) mode | |
| LE3 | Volumetric fog shafts traced by rays when ray traced shadows are on | |
| LE4 | Terrain water: sea level and lakes in the Terrain Studio | |
| LE5 | Water reflections traced by rays | |
| LE6 | Path Traced view mode | |
| LE7 | Cut-outs' holes in ray traced AO and GI | |

## Sources

- Unreal Engine: [Local Fog Volumes](https://dev.epicgames.com/documentation/en-us/unreal-engine/local-fog-volumes-in-unreal-engine); Hardware Ray Tracing, Path Tracer, Lumen, MegaLights and Water System pages in the same documentation.
- NVIDIA: [RTX technology overview](https://developer.nvidia.com/rtx/raytracing); [RTXDI](https://github.com/NVIDIAGameWorks/RTXDI/blob/main/README.md) (ReSTIR direct lighting); [RTXGI](https://github.com/NVIDIAGameWorks/RTXGI/blob/main/docs/QuickStart.md) (DDGI probes); [RTX Path Tracing SDK](https://developer.nvidia.com/rtx/path-tracing/get-started) (RTXPT).
- Axcenity: [hardware-ray-tracing.md](hardware-ray-tracing.md), [global-illumination.md](global-illumination.md), [../rendering/ray-tracing.md](../rendering/ray-tracing.md), [../rendering/fog.md](../rendering/fog.md), [../rendering/lights.md](../rendering/lights.md), [../rendering/water.md](../rendering/water.md).
