# Global illumination improvements (R18): options and plan

Roadmap item R18, section 6.3 of [competitive-development-roadmap.md](competitive-development-roadmap.md). This document says what indirect light does today, which options were weighed against the section's requirements, and the stage plan. Each stage is its own PR. Ray-traced probe GI is RT2 in [hardware-ray-tracing.md](hardware-ray-tracing.md) and is not repeated here. RT2 keeps the probe buffer's layout (four rgb L1 coefficients a probe) and writes into it on the GPU. For setting up the ray traced look, see the Learn guide [ray-tracing.md](../../documentation/learn/ray-tracing.md).

## What exists

Details are in [sky.md](../rendering/sky.md).

- **Light probe volumes** (graphics Phase 19). A `LightProbeVolume` in a Part fills its box with a grid of probes about `Spacing` studs apart, with at most 16 per side. **Bake Probes** draws six 16² faces per probe, one bounce of sun, lamps and sky, and reduces them on the CPU to four spherical harmonics coefficients (L1 SH, rgb). The result is saved as `assets/probes/<part>.lightprobes.json`. The renderer keeps the nearest 8 volumes in one storage buffer, 64 bytes a probe. Every surface in a box blends the eight probes around it at its normal. Smaller boxes come first, then bigger ones, then the sky.
- **Real-time GI** (Phase 20, off at every preset). This re-traces 2 probes a frame through the normal pipeline while the light changes, and rests while it doesn't. A heavy scene draws one face every 4 frames instead. Its cost is a second renderer's copy of the scene plus 12 small scene draws a frame (`performance.md`: 434 of 474 ms in the 36,000-item forest before the heavy-scene path).
- **Stale bakes.** Each bake keeps a fingerprint of the world. The Explorer badges a volume whose bake came from a different world.
- **Reflection probes** (Phase 17) and the sky's own diffuse light (`EnvironmentIntensity`) fill everything else.
- **Missing:**
  - Volumes can't be seen: there's no picture of where probes are, what light each holds, or which ones are bad.
  - Probes inside walls and solid parts see through back faces and leak light. The docs only say "keep walls thicker than the gaps between probes".
  - Without real-time GI, a time-of-day change leaves volumes lit as baked.
  - Placement is by hand, one Part at a time.
  - Real-time GI is only on or off.
  - There are no numbers per volume or per setting.

## Options weighed

| Option | Helps with | Cost | Verdict |
|---|---|---|---|
| **Probe overlay and stats** (draw each probe as a small sphere lit by its SH; flag missing, stale and invalid probes; counts, memory and bake time per volume) | Diagnostics; "why is this room dark" | Editor only, nothing at run time | **Stage 1** |
| **Invalid probes** (a probe inside a solid part is marked at bake and given weight 0, and the blend renormalizes over the valid ones around it) | The most common leak: probes in walls, floors and props | One flag per probe in the bake file and buffer; a few ALU in `surrounding_diffuse` | **Stage 2** |
| **Probe visibility** (DDGI's per-probe depth moments and a Chebyshev test, so a surface ignores probes it can't see) | Leaks through thin walls that invalid probes miss | 8×8 octahedral depth per probe (about 0.5 KB), a second texture lookup per probe per pixel | **Later**, only if the overlay shows leaks after stage 2. RT2 keeps today's four coefficients, but rays would make the depths cheap, while raster bakes would need a seventh pass |
| **Sky light kept apart** (the bake stores the light that came from the sky separately from sun and lamp light; at run time the sky part is scaled by the current sky against the baked one) | Dynamic environment lighting on every GPU: a sky that dims, tints or changes weather relights volumes with no re-tracing | 64 more bytes a probe, in a second buffer beside the probe buffer; one more multiply-add in the shader | **Stage 3**. A moving sun still needs real-time GI or RT2 |
| **Auto placement** (Fill with probes: size a volume to the selection or a model, pick `Spacing` from a probe budget, and skip cells inside solids) | Probe placement and management | Editor only | **Stage 4** |
| **Quality tiers for real-time GI** (probes per frame and face size by preset instead of on or off) | Scalable quality settings | Already measured per probe; a setting, not new code | **Stage 4** |
| **L2 SH** (9 coefficients instead of 4) | Sharper indirect light at corners | 2.25× probe memory and bandwidth; every bake must be redone | Not now: an L1 artefact hasn't been seen in a project, and probe spacing, not SH order, limits detail |
| **Screen-space GI** | Short-range bounce between on-screen surfaces | 1–2 ms plus a denoiser in a forward renderer; misses off-screen light | No: RT2 does better on GPUs that can afford either |
| **Lightmaps** | Exact static indirect light | Lightmap UVs for every part and model, a UV packer, a baker | No: Phase 19 left them out unless volumes prove too coarse, and stages 1–3 make volumes much better first |
| **SDFGI, Lumen or a voxel cone tracer** | Fully dynamic GI | A second scene representation | No: the roadmap says not to attempt this |

## Decisions

- **D1. The probe volume stays the one GI structure.** Every improvement here, and RT2, reads and writes the same volumes and buffer. A project made today keeps working, and a volume baked before a stage still loads: a missing flag means valid, and missing sky light means "all baked light is fixed".
- **D2. Diagnostics come first.** The other stages are judged by what the overlay shows on the forest and on Paul's projects. Stage 2's leak test also uses the overlay's invalid count.
- **D3. Invalid means inside a solid.** At bake time, a probe whose point is inside a `CanCollide` part's shape is invalid: a block, wedge or cylinder by its shape test, and a MeshPart by its collision mesh through `physics`. If the shape test misses real cases (props without collision), stage 2 can add a back-face count from the bake's faces. `surrounding_diffuse` drops invalid probes and renormalizes the eight weights. With all eight invalid, the volume yields to the next volume or the sky.
- **D4. Sky light is a separate set of coefficients, not a visibility function.** The bake draws once with the sky only and once with everything, and stores the sky part as its own four coefficients, in a buffer beside the probe buffer, so the probe buffer's layout (which RT2 writes into) doesn't change. At run time that part is scaled per channel by the current sky's SH band 0 over the baked sky's. This tracks a sky's brightness and tint (time of day, weather, `EnvironmentIntensity`) exactly where the sky's shape stays alike. It doesn't track a sun that moves; that remains real-time GI's job, or RT2's.
- **D5. Every stage is measured.** `bench_light_probes` is a new ignored release bench on the PC. It times bake per probe, the frame cost of 8 volumes of 16³ probes against none, and real-time GI per probe at each tier. Results go in `docs/rendering/performance.md`, so "performance costs are measurable" is a number per setting.

## Stages

| Stage | What | Tests | PC check |
|---|---|---|---|
| R18.1 Diagnostics | A viewport overlay that draws probes as spheres lit by their SH, with missing and stale bakes flagged; a legend with probes, volumes drawn, buffer memory and bake age; per-volume stats in Properties | Overlay builds from a baked volume; stale and missing marks; legend lines | Overlay on the forest |
| R18.2 Invalid probes (done) | Invalid flag at bake (D3; `physics::inside_solids`: colliding parts' shapes, MeshParts' hulls, under a Terrain's ground, never the volume's own part), stored in the bake file (optional list, older files load); `surrounding_diffuse` renormalizes; overlay shows invalid probes | A wall with probes inside no longer lights the next room (image test); old bakes draw as before | Leak check in a real interior |
| R18.3 Sky light apart (done) | Second bake pass without the sky (half size, 8²) whose difference is the sky's share, stored as optional `sky` (`band0` and four coefficients a probe) in the bake file and after the probes in the same buffer (D4); each `Frame::volumes` entry carries the current over baked sky band 0, minus one; probes traced live (second `w` 1) get nothing added; the overlay legend counts bakes without it | A sky dimmed to half takes away half the sky's share (image test, linear light); old bakes and a sky as baked draw unchanged | Day-night in a baked house without real-time GI |
| R18.4 Placement and tiers (done) | **Fill with Probes** (Light menu, palette): a volume in a new invisible part around the selected parts or model, plus 2 studs a side, `Spacing` the closest half stud within 512 probes, probes inside solids counted in Output and left out by the bake (R18.2); real-time GI tiers by preset (`RealtimeGiTier`: Low 1 probe a frame at 8², Medium 2 at 8², High 2 at 16², Ultra 4 at 16²); `bench_light_probes` (bake per probe with and without the sky apart, 8 volumes of 16³ against none, real-time GI per tier) | Fill sizes a volume to a model within the budget and counts the probes in its pillar; the budget's spacing; each tier traces its probes a frame and lands near a fresh bake | `bench_light_probes` numbers |
| RT2 (ray tracing thread) | DDGI-style update into the same volumes and the same four coefficients a probe; it should skip invalid probes once R18.2 lands | Its own | Its own |

Each stage updates [sky.md](../rendering/sky.md), the wiki's Lighting-and-Atmosphere page, and the roadmap's 6.3 status.

## Acceptance (section 6.3)

- **Existing lighting projects remain functional:** D1. Old bakes load and draw the same way.
- **New techniques can be enabled selectively:** invalid probes and sky light apart are on per bake (re-bake to get them). Real-time GI tiers and RT2 are machine settings.
- **Performance costs are measurable:** D5.
- **Low-end hardware remains supported:** stages 1–3 add nothing that needs compute or ray tracing. Real-time GI stays opt-in.

## Risks

- **Two bake passes double bake time.** The sky-only pass can draw at 8² instead of 16², since sky light varies slowly. Measure in stage 3.
- **A wrong invalid flag darkens a room.** Keep the back-face fraction threshold conservative, show invalid probes in the overlay, and allow `LightProbeVolume.KeepInsideProbes` if a real project needs it.
- **Probe memory grows.** Stage 3 adds 64 bytes a probe, after the probes in the same buffer (so the probes' own layout, which RT2 writes, doesn't change). Eight volumes of 16³ go from 2 MB to 4 MB in all, still small beside textures.
