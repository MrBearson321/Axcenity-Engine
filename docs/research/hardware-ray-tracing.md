# Optional hardware ray tracing: research and plan

Paul's request (2026-11-12): hardware ray tracing as an **optional GPU feature** that capable hardware can turn on, choosing **the most performant option that still looks good**. This document checks what wgpu 30 can do, measures it on the owner's RX 9070 XT, compares the effects and how other engines ship them, and ends with a phased plan (RT0–RT5) and a recommended first effect.

It overrides the "Ray tracing: Deferred" row of the [competitive development roadmap](competitive-development-roadmap.md) (section 11.1) and ties into R18 (global illumination); see [How this fits the roadmap](#how-this-fits-the-roadmap).

**Recommendation (2026-11-12).**

1. Build on wgpu's **inline ray queries** (`Features::EXPERIMENTAL_RAY_QUERY`), not ray tracing pipelines. Ray queries work on DX12, Vulkan and Metal in wgpu 30.0.1; pipelines are Vulkan only.
2. **First effect: ray-traced sun shadows** (one jittered ray per pixel from the prepass, plus a shadow denoiser). It costs **0.25–0.30 ms of rays at 1440p** on the RX 9070 XT, it removes the cascade artifacts and the 500-stud caster limit, it slots into the forward renderer without a new texture binding, and it builds the shared foundation every later effect needs.
3. **Second effect: ray-traced light probes** (DDGI-style) replacing the real-time GI's raster cube faces. This is the best quality per millisecond for R18: rays are cheap here, and today's real-time GI draws the whole scene six times per probe.
4. Then ray-traced AO, then hybrid reflections. **Not** ReSTIR DI/GI or path tracing: 8–18 ms on high-end GPUs in the published numbers, too much for an optional tier.
5. Everything stays **off by default**, offered only when the GPU has ray queries, with today's renderer (cascaded PCSS, SSAO, baked and real-time probes) as the exact fallback.

## Contents

- [What wgpu 30 offers](#what-wgpu-30-offers)
- [Measured on the RX 9070 XT](#measured-on-the-rx-9070-xt)
- [Which effects give the most per millisecond](#which-effects-give-the-most-per-millisecond)
- [Denoising](#denoising)
- [Upscaling and TAA](#upscaling-and-taa)
- [Acceleration structures for a scene of Instances](#acceleration-structures-for-a-scene-of-instances)
- [Fallback for GPUs without ray tracing](#fallback-for-gpus-without-ray-tracing)
- [How other engines do it](#how-other-engines-do-it)
- [Fitting it into the renderer](#fitting-it-into-the-renderer)
- [Phased plan](#phased-plan)
- [How this fits the roadmap](#how-this-fits-the-roadmap)
- [Risks](#risks)
- [Sources](#sources)

## What wgpu 30 offers

Read from the wgpu 30.0.1 source the engine builds (`wgpu`, `wgpu-types`, `wgpu-hal`, `wgpu-core`, `naga`, all 30.0.1 in `Cargo.lock`).

| Feature | What it gives | Backends that expose it in 30.0.1 | Where |
|---|---|---|---|
| `EXPERIMENTAL_RAY_QUERY` | Bottom- and top-level acceleration structures (BLAS, TLAS), `acceleration_structure` bindings, and `ray_query` in **any** shader stage (compute, fragment, vertex) | **DX12**: raytracing tier 1.1 and Shader Model 6.5. **Vulkan**: `VK_KHR_acceleration_structure` + `VK_KHR_ray_query` (+ buffer device address, deferred host operations). **Metal**: macOS 15 / iOS 18 and a device that `supportsRaytracing` and `supportsRaytracingFromRender` | `wgpu-hal/src/dx12/adapter.rs` (`supports_ray_tracing`), `vulkan/adapter.rs` (`supports_ray_query`), `metal/adapter.rs` (`supports_raytracing`) |
| `EXTENDED_ACCELERATION_STRUCTURE_VERTEX_FORMATS` | BLAS vertices as `Float16x2/x4`, `Snorm16x2/x4`, `Float32x2` (otherwise only `Float32x3`) | DX12, Vulkan (set together with ray queries) | same |
| `ACCELERATION_STRUCTURE_BINDING_ARRAY` | `binding_array<acceleration_structure>` | DX12, Vulkan | same |
| `EXPERIMENTAL_RAY_HIT_VERTEX_RETURN` | The hit triangle's positions returned by the query | Vulkan only | `vulkan/adapter.rs` |
| `EXPERIMENTAL_RAY_TRACING_PIPELINES` | Ray generation / hit / miss shaders and shader binding tables | **Vulkan only**; Metal's hal says "Ray tracing pipelines are unsupported", DX12's says "once ray tracing pipelines are supported they also will go here" | `metal/device.rs`, `dx12/adapter.rs` |

Facts that shape the design:

- **Experimental.** Using any `EXPERIMENTAL_*` feature needs `DeviceDescriptor::experimental_features: unsafe { ExperimentalFeatures::enabled() }`; wgpu warns of "major bugs" and "breaking changes" (`wgpu-types/src/features.rs`, `tokens.rs`). API feedback goes to gfx-rs/wgpu issues #1040 and #6762.
- **WGSL.** `enable wgpu_ray_query;`, then `var rq: ray_query; rayQueryInitialize(&rq, tlas, RayDesc(flags, mask, tmin, tmax, origin, dir)); rayQueryProceed(&rq); rayQueryGetCommittedIntersection(&rq)`. Ray flags (`RAY_FLAG_TERMINATE_ON_FIRST_HIT`, `RAY_FLAG_FORCE_OPAQUE`, `RAY_FLAG_CULL_*`, ...) and `RayIntersection` fields (`t`, `instance_custom_data`, `primitive_index`, `barycentrics`, `world_to_object`, ...) are in naga's WGSL front end. Candidate hits (for alpha testing) use `rayQueryConfirmIntersection` in a `rayQueryProceed` loop.
- **No refit.** `AccelerationStructureUpdateMode::PreferUpdate` exists, but wgpu-core logs "only rebuild is implemented" and always does a full build, for BLAS and TLAS (`wgpu-core/src/command/ray_tracing.rs`). A moving instance means a TLAS rebuild; a deforming mesh means a BLAS rebuild.
- **TLAS instances live on the CPU.** `Tlas` holds `Vec<Option<TlasInstance>>` (a 3×4 transform, 24-bit custom data, 8-bit mask, a `Blas`); a build uploads from `lowest_unmodified()` up, so instances that never change should sit at low indices. Writing instances from the GPU needs the hal escape hatch (`CommandEncoder::as_hal_mut` plus `unsafe mark_acceleration_structures_built`), per backend.
- **Compaction** works (`Blas::prepare_compaction_async`, `Queue::compact_blas`).
- **Shader runtime checks.** naga tracks ray query initialization by default (`ray_query_initialization_tracking`); `create_shader_module_trusted` with unchecked runtime checks removes it. The probe below used the checked default.
- **Limits.** `Limits::default()` allows zero acceleration structures; the device must be opened with the adapter's ray tracing limits (`max_tlas_instance_count`, `max_blas_primitive_count`, `max_acceleration_structures_per_shader_stage`). The RX 9070 XT reports 16,777,216 instances and 536,870,912 primitives.
- **Cloud testing works.** The cloud container's lavapipe (Mesa 25.2.8) exposes `VK_KHR_acceleration_structure` and `VK_KHR_ray_query`; the probe below runs there, so ray traced image tests can run without a GPU.
- **DX12 needs DXC**, which the engine already links statically (`wgpu` feature `static-dxc`, PR #172).

Which GPUs that covers: DX12 raytracing tier 1.1 means NVIDIA RTX 20 and later, AMD RX 6000 (RDNA2) and later, Intel Arc. On Metal, Apple silicon reports ray tracing support; Apple added hardware-accelerated ray tracing with M3 ([Apple, M3](https://www.apple.com/newsroom/2023/10/apple-unveils-m3-m3-pro-and-m3-max-the-most-advanced-chips-for-a-personal-computer/)), so M1/M2 run it more slowly.

## Measured on the RX 9070 XT

A stand-alone probe, `benchmarks/ray-tracing-probe` (its own workspace, same wgpu 30.0.1 with `static-dxc`), times BLAS builds, TLAS rebuilds and one ray per pixel for each effect through compute shaders with inline ray queries. The primary pass stands in for the prepass (it writes each pixel's position and normal); every other pass starts from those pixels, as a hybrid renderer would. Timings are GPU timestamps, medians of 30 frames. Rays only: **no shading at hits, no alpha testing, no denoising**.

Run on Paul's PC, 2026-11-12: AMD Radeon RX 9070 XT, DX12 driver 32.0.32015.7029, Vulkan driver 26.10.1 (LLPC), Windows 10.0.26300. Scene: 10,201 instances (a ground plane, 10,000 cubes, 200 spheres of 100,352 triangles; 20.1 M triangles), dense enough that 58% of visible ground is in shadow. Camera at 2560×1440 unless noted.

### Ray passes (GPU ms)

| Pass (one ray per pixel unless noted) | Rays | DX12 1440p | DX12 1440p repeat | Vulkan 1440p | DX12 1080p | DX12 1440p, 101,001 instances |
|---|---|---|---|---|---|---|
| Primary (nearest hit, then the hit's normal) | 3.69 M | 0.470 | 0.584 | 0.517 | 0.319 | 0.848 |
| Sun shadow, hard | 3.69 M | **0.251** | 0.299 | 0.272 | 0.148 | 0.361 |
| Sun shadow, soft (jittered in a 1.5° cone) | 3.69 M | **0.251** | 0.303 | 0.276 | 0.150 | 0.367 |
| Ambient occlusion (cosine ray, 2 studs) | 3.69 M | **0.261** | 0.314 | 0.283 | 0.158 | 0.433 |
| Mirror reflection, half resolution (nearest hit) | 0.92 M | 0.140 | 0.171 | 0.153 | 0.090 | 0.219 |
| Diffuse GI, half resolution (cosine ray to nearest hit, then a sun ray from there) | 1.84 M | 0.197 | 0.236 | 0.218 | 0.138 | 0.305 |

Shadow and AO rays run at 12–15 billion rays a second, nearest-hit rays at 5–8 billion. Ten times the instances (100,000 cubes, 1,000 spheres, 110 M triangles) costs 1.4–1.7× per pass. The repeat run's rays were about 20% slower than the first; treat the range as the number. Vulkan was 5–10% slower than DX12, matching the engine's choice of DX12 on Windows.

### Acceleration structure builds

| BLAS | Triangles | DX12 GPU ms | Vulkan GPU ms |
|---|---|---|---|
| Cube | 12 | 0.05 | 0.11 |
| Sphere | 100,352 | 0.46 | 0.93–1.12 |
| Sphere | 999,698 | 2.56–2.60 | 4.08–4.13 |

`PREFER_FAST_BUILD` and `PREFER_FAST_TRACE` built in the same time on this driver (the first run was 40% slower while the GPU clocked up). About **2.6 ms per million triangles** on DX12.

| TLAS rebuilt with every instance moved | CPU in wgpu (place + encode + submit) | GPU build |
|---|---|---|
| 1,000 | 0.13 ms | 0.13–0.20 ms |
| 10,000 | 1.4–1.6 ms | 0.17–0.25 ms |
| 100,000 | 14.1–14.8 ms | 0.72–1.11 ms |

The GPU build is cheap; **wgpu's CPU side is not**. A cloud rerun with the timing split puts about 85% of that CPU time in wgpu's encode and submit (instance conversion and validation) and 15% in the probe's own placing loop. Moving all 100,000 instances every frame would take most of a 60 fps frame's 16.7 ms, so the TLAS must only rewrite what moved (see [Acceleration structures](#acceleration-structures-for-a-scene-of-instances)).

### What the numbers mean

- At 1440p, a ray per pixel for sun shadows or AO is **about 0.25–0.3 ms**. Today's heavy scenes spend **5.7 ms on shadow-map vertices** (forest at Ultra, [Rendering](../../wiki/Engine-Systems/Rendering.md)); shadows were 43–52% of the GPU frame before culling.
- The ray cost is not the effect's cost: add the denoiser (about 0.4 ms, below), alpha testing for cut-out leaves (about +20% per ray, below), and for reflections and GI the shading at each hit.

## Which effects give the most per millisecond

Totals for 1440p on an RX 9070 XT. "Rays" are measured above; the rest are estimates from the sources listed, marked as such.

| Effect | What it fixes | Rays (measured) | Denoise / extra (estimated) | Likely total | Verdict |
|---|---|---|---|---|---|
| **Sun shadows**, 1 ray/pixel, soft | Cascade seams, acne, peter-panning, blurry far shadows, the 500-stud caster range; true contact-hardening penumbrae | 0.25–0.30 ms | Shadow denoiser ~0.4 ms (NRD SIGMA 0.40 ms on an RTX 4080 at 1440p); alpha-tested casters ~+20% rays | **~0.7–0.9 ms**, and the sun's cascades can be dropped | **First**: cheapest, most visible, easiest to integrate |
| **Probe GI by rays** (DDGI-style, updating the existing light probe volumes) | Dynamic indirect light that follows the sun, lamps and moving parts; replaces today's 6 raster scene draws per probe | ~0.15–0.2 ms for 128 rays × 8,192 probes (about 1 M rays at the measured nearest-hit rate) | Hit shading from a material table, probe blending; DDGI's whole update was 2.6 ms on an RTX 2080 Ti in 2019 | **~0.5–1 ms** (estimate), can spread over frames | **Second**: the R18 payoff |
| **Ambient occlusion**, 1 ray/pixel | SSAO's screen-edge and thin-wall errors; occlusion from off-screen geometry | 0.26–0.31 ms | Denoiser ~0.4–0.5 ms | ~0.7–0.8 ms | Third; shares the shadow denoiser's infrastructure |
| **Reflections**, half res, roughness cutoff, probes for misses and rough surfaces | Correct mirrors, water and metal reflecting moving things | 0.14–0.17 ms | Hit shading plus denoise and upscale: AMD's GI-1.0 reflections took 0.46–0.96 ms for denoise + upscale on an RX 7900 XTX at 1080p | ~1–2 ms | Fourth: costs more because every hit is shaded |
| Full-screen ReSTIR GI / DI, path tracing | Best quality | — | ReSTIR GI added 8–18 ms in UE4 on an RTX 3090 at 1080p; ReSTIR DI 15 ms | 8 ms or more | **No** |

Why shadows first:

- Lowest cost, and the result can replace work the frame already does (the sun's cascade passes).
- One ray per pixel is all it needs; soft shadows come from jittering inside the sun's cone (identical cost, measured) and denoising.
- It needs nothing the forward renderer lacks: the prepass already has depth and normals (`OcclusionTargets`), and TAA's motion vectors exist for the denoiser.
- It builds what every later effect reuses: acceleration structures from the draw list, the capability and setting, the denoiser, ray traced tests on lavapipe.

AMD's **Hybrid Shadows** goes further: classify 8×4 tiles with the cascades and trace only uncertain (penumbra) pixels. AMD publishes no millisecond savings for it. With rays at 0.25–0.3 ms the classifier only pays where it skips sky and back-facing pixels cheaply; keep it as an option after measuring.

## Denoising

One ray per pixel is noisy; every effect needs a temporal plus spatial filter.

| Denoiser | Covers | Cost | License | Fit |
|---|---|---|---|---|
| **AMD FidelityFX Shadow Denoiser** | 1 ray/pixel soft shadows; bit mask per 8×4 tile, tile classification, temporal then spatial passes | not published | **MIT** | Port to WGSL for RT1 (plain compute shaders) |
| AMD FidelityFX Reflection Denoiser | Roughness-guided reflection filtering | not published | MIT | Port for RT4 |
| NVIDIA NRD (SIGMA / REBLUR / RELAX) | Shadows / diffuse + specular / ReSTIR signals | 0.40 / 2.55 / 3.25 ms, RTX 4080, 1440p | NVIDIA RTX SDKs License (not open source) | Reference for costs only; its license doesn't fit an Apache-2.0 engine shipping modified shaders |
| SVGF, A-SVGF | Path-traced GI | ~10 ms / 2 ms at 1080p on older GPUs | Papers | Ideas for the GI denoiser |
| AMD FSR Ray Regeneration, NVIDIA DLSS Ray Reconstruction | ML denoisers | — | Vendor SDKs, vendor-specific | Out of scope (vendor SDKs, no WGSL path) |

Plan: port the FidelityFX shadow denoiser to WGSL in RT1 (MIT; credit it in `NOTICE`), reuse its tile classification and temporal reprojection for AO in RT3, and the reflection denoiser in RT4. Probe GI (RT2) needs no screen-space denoiser: probes average many rays over time (hysteresis), as DDGI does.

## Upscaling and TAA

- **Trace at the internal resolution**, after the prepass and before lighting, as Lumen does before TSR. A render scale below 100% cuts rays in proportion.
- **Denoise fully before shading**. TAA holds history to the 3×3 neighbourhood's colours (`temporal.md`), so noisy visibility left for TAA to clean would be clamped and flicker. The denoiser uses the same motion vectors TAA uses (`TemporalTargets`), so ray tracing keeps the motion targets alive the way TAA does.
- **Today's upscaler** (G9 stage 1, EASU + RCAS after the frame) sees only the finished image and needs nothing from ray tracing. **G9 stage 2** (temporal upscaling inside TAA) is independent and complementary; the denoiser must use non-jittered positions, as NRD requires.
- **Reduced-resolution effects** (half-res AO, reflections) upsample themselves with a depth-aware filter, as Hybrid Reflections (FSR 1 upscale) and GI-1.0 (its own temporal upscale) do, since there is no joint denoise-and-upscale here.

## Acceleration structures for a scene of Instances

What the measurements and wgpu's design imply:

- **BLAS per mesh, built once.** Built when a mesh uploads, `PREFER_FAST_TRACE`, then compacted (RDNA4 needs about 48 bytes per triangle compacted; 26 on an RTX 4090). Built-in shapes share a handful. 2.6 ms per million triangles means a large import builds in a few frames; spread builds over frames with a budget, like texture uploads.
- **LOD.** One LOD per mesh for ray tracing (a coarser one for big meshes, as Unreal and Far Cry 6 do); shadows and AO don't need the full mesh.
- **Skinned and deforming meshes.** wgpu has no refit, so each pose is a full BLAS rebuild (about 0.1 ms for a 20k-triangle character, since small builds cost about 0.05 ms however few triangles they have, plus a skinning compute output in a BLAS-input buffer). Budget the rebuilds per frame (Far Cry 6 capped them at 16 a frame, 12 skinned) and let the rest reuse last frame's.
- **Terrain** goes in as chunked BLASes (large overlapping instances slow traversal).
- **TLAS every frame, but write only what moved.** wgpu re-uploads instances from the lowest modified index up, so keep stable slots: static instances first, moving ones at the end; a slot only changes when its item moves or leaves. With few moving parts this costs well under the 1.6 ms (10k) measured for moving everything. Past ~100,000 instances (Epic's guideline for good performance too) cull: only items within a radius of the camera (Far Cry 6: 100 m) and above a size.
- **Masks.** Instance mask bits per kind (solid, cut-out, see-through, water, terrain, sprite-like) so each effect skips what it shouldn't hit.
- **Cut-out foliage.** Hardware traversal stops for each non-opaque candidate, about +20% per ray (The Riftbreaker on an RX 6800 XT). Mark everything opaque except cut-out materials, and alpha-test those in the `rayQueryProceed` loop against their opacity texture.
- **Later, if the CPU side bites:** build the TLAS instance buffer on the GPU through the hal escape hatch, or contribute GPU-written instances or refit upstream to wgpu.

## Fallback for GPUs without ray tracing

The fallback is today's renderer, unchanged: cascaded PCSS sun shadows, SSAO, baked light probe volumes and reflection probes, and the optional real-time probe GI. Following ADR-0013 (look versus cost):

- `Capabilities` gains `ray_queries` (the adapter has `EXPERIMENTAL_RAY_QUERY` and isn't software).
- `GraphicsSettings` gains a ray tracing setting, **off at every preset**. Each effect is its own switch once it exists (Sun shadows, Probe GI, AO, Reflections), so a machine turns on only what it can afford.
- `effective` turns it off with a note when the GPU can't, as it does for MSAA and shadow sizes. The editor and player show it only when supported.
- Off means no acceleration structures, no passes, no memory: frames are identical to today's (`hello-graphics` references must still match).

A software fallback (compute-shader BVH, like Unity's UnifiedRayTracing or Lumen's distance fields) is **not** recommended: the existing probes and cascades already cover non-RT GPUs, and a second tracing path doubles the work.

## How other engines do it

| Engine | Hardware ray tracing | Effects | Fallback |
|---|---|---|---|
| **Unreal 5** | Optional; needs inline ray tracing and an RTX 20 / RX 6000 class GPU | Lumen GI and reflections (screen traces first, then hardware rays on triangles); MegaLights (experimental, needs HW RT, replaces ray traced shadows); RT shadows and RTAO (AO only without Lumen) | Lumen with software tracing against distance fields; virtual shadow maps |
| **Godot 4** | Vulkan-only ray tracing plumbing in RenderingDevice since 4.7 dev 1 (pipelines, not queries); no built-in effect as of 4.8 beta 1 | None shipped (an RTAO demo) | SDFGI and VoxelGI, no ray tracing hardware needed |
| **Unity** | HDRP only, DX12 and consoles | HDRP: RT shadows, AO, GI, reflections, contact shadows, recursive rendering, SSS. URP: none (roadmap "under consideration"). Surface Cache GI (preview, 2026) runs on URP and HDRP | Screen-space effects, light probes, APV; Surface Cache GI falls back to a software BVH |
| **s&box** | None found | SSR, dynamic reflections, indirect light volumes | (It is the fallback) |
| **Axcenity (proposed)** | Optional ray queries on DX12, Vulkan and Metal | Sun shadows, then probe GI, AO, reflections | Today's cascades, SSAO and probes |

The common pattern: ray tracing is an opt-in tier on top of a raster renderer, hybrid (screen space or probes first, rays where they help), with the raster path kept as the fallback. Unreal and AMD both start from shadows and diffuse GI; full ReSTIR and path tracing stay in "cinematic" modes.

## Fitting it into the renderer

- **ADR-0003 holds.** Acceleration structures are built inside `render` from the `DrawList` (mesh ids, transforms, materials); nothing reads Instances.
- **Forward renderer.** Ray traced passes are compute passes between the prepass and the scene pass, reading the prepass depth and normals. Their result reaches the scene shader through the **screen occlusion binding** (frame binding 7, today one `R8Unorm` channel of SSAO at half size): with ray tracing on it becomes a full-size two-channel texture, AO in red, sun visibility in green, so the item shader's 16 sampled textures stay 16. `sun_visibility` in `lighting.wgsl` reads it instead of the cascades.
- **Material table for hits.** Probe GI and reflections shade hits from a storage buffer indexed by `instance_custom_data`: base colour (a texture's average until bindless textures), emissive, metalness, roughness. Cut-out leaves need their opacity texture: a texture binding array (`TEXTURE_BINDING_ARRAY` with non-uniform indexing, on all three backends) or an atlas.
- **Timing.** Each pass gets a `FramePass` in `gpu_timer.rs`, and the TLAS build a CPU span, so `profile_project` and the benchmarks see them.
- **Real-time GI today** (`realtime_gi.rs`) draws six faces of the whole scene per probe, two probes a frame, and one face every fourth frame past 4,000 items. Rays replace those draws one for one (RT2).

## Phased plan

Each phase is one PR from `main`, with GPU tests (lavapipe in the cloud, the RX 9070 XT on Paul's PC) and a benchmark before and after.

| Phase | Work | Done when |
|---|---|---|
| **RT0 Foundation** | `Capabilities::ray_queries`; open the device with the feature and limits when present (player, editor, `TextureRenderer`); ray tracing setting (off at every preset, `effective` notes); `renderer/ray_tracing.rs`: BLAS per mesh (built on upload with a per-frame budget, compacted), TLAS from planned items with stable slots and masks; a "Ray hits" debug view; ADR for the choice of ray queries | Turning it on builds the structures and draws the debug view; off is byte-identical to today; TLAS CPU and GPU time in the profiler; S10k and a heavy project measured |
| **RT1 Sun shadows** (recommended first effect) | Compute pass: one ray per pixel in the sun's cone (the Lighting sun's size sets softness); WGSL port of the FidelityFX shadow denoiser using TAA's motion vectors; visibility into the screen occlusion texture; `sun_visibility` uses it and the sun's cascades are skipped; cut-out casters alpha-tested in the query loop; point and spot lights keep their maps | Image tests against references; the forest and S10k at 1440p Ultra cheaper or within 1 ms of PCSS; no shimmer in motion (hand check) |
| **RT2 Probe GI** (R18) | DDGI-style update of the existing light probe volumes: N rays per probe, hit shading from the material table plus a sun ray, multi-bounce from last frame's probes, hysteresis blend into the probe buffer; replaces the raster faces in `realtime_gi.rs` when ray tracing is on | A sunset and a moving lamp relight a room within a second; the GPU cost per frame stays under 1 ms at 1440p on the RX 9070 XT |
| **RT3 Ambient occlusion** | One cosine ray per pixel (short), denoised with RT1's infrastructure; replaces SSAO while on | Image tests; ≤ 1 ms at 1440p |
| **RT4 Reflections** | Half-res rays below a roughness cutoff (GI-1.0 used 0.2), hits shaded from the material table, probe GI and a sun ray; misses and rough surfaces fall back to reflection probes and the environment; reflection denoiser port; water uses it | Mirror and water tests; ~1–2 ms at 1440p |
| **RT5 Scale** (only if measurements need it) | Skinned BLAS rebuild budget; GPU-written TLAS instances through the hal or upstream; hybrid shadow tile classifier; disabling naga's ray query runtime checks once stable | Driven by a measured bottleneck |

Not planned: ReSTIR DI/GI, ray tracing pipelines (Vulkan only in wgpu), NRD, DLSS or FSR ray reconstruction. Path tracing was added as an editor view mode on 2026-11-13 ([lighting-engine-audit.md](lighting-engine-audit.md), LE6).

## How this fits the roadmap

- **Competitive roadmap, section 11.1:** the "Ray tracing: Deferred" row becomes "Planned, optional (RT0–RT5, this document)". Paul overrode the deferral on 2026-11-12.
- **R18 (global illumination):** RT2 is R18's "evaluate additional real-time GI techniques" for GPUs that have ray tracing. R18's other work (probe placement, diagnostics, quality settings, the non-RT path) still applies to every GPU, and RT2 writes into the same light probe volumes, so R18's improvements help both.
- **G9 stage 2 (temporal upscaling):** independent; RT denoisers run before TAA (see [Upscaling and TAA](#upscaling-and-taa)).
- **L5 (GPU-driven drawing):** not needed for RT0–RT4. GPU-written TLAS instances (RT5) would share its GPU-side instance data.

## Risks

- **wgpu ray tracing is experimental**: bugs and breaking API changes between wgpu releases. Keep the code in one module behind the capability, with tests that run on lavapipe so an upgrade shows breakage.
- **wgpu's TLAS CPU cost** (14 ms per 100,000 moved instances on a Ryzen 9 9950X3D): stable slots and culling keep it small; the hal path is the way out.
- **No refit**: deforming meshes rebuild; budget them.
- **Memory**: about 48 bytes per triangle on RDNA4 after compaction; 20 M unique triangles would be about 1 GB, so very dense projects need RT LODs.
- **Driver variance**: the first run on the PC was slower on builds and the repeat slower on rays; benchmark with several runs and medians.
- **Apple M1/M2** report ray tracing without hardware acceleration; the setting stays off by default, so it costs nothing unless turned on.

## Sources

Measurements: `benchmarks/ray-tracing-probe` (this PR), five runs on Paul's PC on 2026-11-12. wgpu facts: the wgpu 30.0.1 crates' source (files named above).

- AMD, *GI-1.0: A Fast and Scalable Two-level Radiance Caching Scheme* (2022): https://gpuopen.com/download/GPUOpen2022_GI1_0.pdf ; arXiv https://ar5iv.labs.arxiv.org/html/2310.19855
- AMD, *Real-Time Reflection* (SIGGRAPH Asia 2023): https://gpuopen.com/download/SA2023_RealTimeReflection.pdf
- AMD, FidelityFX Hybrid Shadows: https://gpuopen.com/download/GPUOpen_Hybrid_Shadows.pdf , https://gpuopen.com/fidelityfx-hybrid-shadows/ ; classifier: https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/blob/main/docs/techniques/classifier.md
- AMD, FidelityFX Hybrid Reflections: https://gpuopen.com/fidelityfx-hybrid-reflections/
- AMD, FidelityFX Denoiser: https://gpuopen.com/manuals/fidelityfx_sdk/techniques/denoiser/ , https://github.com/GPUOpen-Effects/FidelityFX-Denoiser (MIT)
- AMD, Brixelizer GI (GDC 2024): https://gpuopen.com/download/GDC2024_GI_with_AMD_FidelityFX_Brixelizer.pdf
- AMD, Far Cry 6 hybrid reflections (GDC 2022): https://gpuopen.com/download/GDC_Performant_Reflective_Beauty_Hybrid_Ray_Traced_Reflections_In_Far_Cry_6.pdf
- AMD, RDNA performance guide: https://gpuopen.com/learn/rdna-performance-guide/ ; RRA guidance: https://gpuopen.com/learn/improving-rt-perf-with-rra/
- AMD, RDNA4 at Hot Chips 2025: https://hc2025.hotchips.org/assets/program/conference/day1/8_amd_pomianowski_final.pdf ; Radeon Raytracing Analyzer for RX 9000: https://gpuopen.com/learn/radeon_raytracing_analyzer_v1-8_support_for_rx9000_series_gpus_and_more/
- Chips and Cheese on RDNA4 ray tracing: https://old.chipsandcheese.com/?p=35892
- Arseny Kapoulkine, *Measuring acceleration structures* (2025): https://zeux.io/2025/03/31/measuring-acceleration-structures/
- Majercik et al., *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields* (2019): https://cim.mcgill.ca/~derek/files/DDGI-lowres.pdf ; NVIDIA RTXGI: https://developer.nvidia.com/blog/rtx-global-illumination-part-i
- NVIDIA, *Best practices for using NVIDIA RTX ray tracing (updated)*: https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/
- NVIDIA NRD: https://www.github.com/NVIDIAGameWorks/RayTracingDenoiser ; license https://raw.githubusercontent.com/NVIDIA-RTX/NRD/master/LICENSE.txt
- Bitterli et al., ReSTIR (2020): https://cs.dartmouth.edu/wjarosz/publications/bitterli20spatiotemporal.html ; Ouyang et al., ReSTIR GI (2021): https://diglib.eg.org/bitstream/handle/10.1111/cgf14378/v40i8pp017-029.pdf
- Schied et al., SVGF (2017): https://cg.ivd.kit.edu/publications/2017/svgf/svgf_preprint.pdf ; A-SVGF: https://cg.ivd.kit.edu/atf.php
- The Riftbreaker ray traced shadows: https://gamedeveloper.com/design/raytraced-shadows-implementation-in-the-riftbreaker
- Microsoft DirectX Raytracing spec: https://microsoft.github.io/DirectX-Specs/d3d/Raytracing.html
- Unreal Engine: Lumen technical details https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-technical-details-in-unreal-engine , Lumen performance guide https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-performance-guide-for-unreal-engine , hardware ray tracing https://dev.epicgames.com/documentation/unreal-engine/hardware-ray-tracing-in-unreal-engine , MegaLights https://dev.epicgames.com/documentation/unreal-engine/megalights-in-unreal-engine?lang=en-US
- Godot: 4.7 dev 1 https://godotengine.org/article/dev-snapshot-godot-4-7-dev-1/ , 4.8 beta 1 https://godotengine.org/article/dev-snapshot-godot-4-8-beta-1/ , SDFGI https://docs.godotengine.org/en/4.7/tutorials/3d/global_illumination/using_sdfgi.html
- Unity: HDRP ray traced effects https://docs.unity3d.com/Packages/com.unity.render-pipelines.high-definition@16.0/manual/lighting-ray-traced-effects.html , URP ray tracing roadmap https://activation.unity3d.com/roadmap/2521-urp-ray-tracing-support , UnifiedRayTracing https://docs.unity3d.com/Packages/com.unity.render-pipelines.core@17.3/manual/UnifiedRayTracing/unified-ray-tracing-api.html
- s&box documentation index: https://sbox.game/llms.txt
