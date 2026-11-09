# Rendering performance

How drawing is measured, and the baseline that optimizations (culling, LOD, GPU skinning, occlusion) are compared against. Measure before optimizing: a change that claims to make drawing faster shows it here.

## What's measured

- **`RenderStats`** (`TextureRenderer::stats`, `render/src/renderer.rs`), for the last frame: draw calls, triangles and vertices (the drawn meshes' corners, every instance counted; shadow views and the prepass included); items submitted (in the draw list), drawn (by the scene pass) and culled (outside the camera's view); shadow casters drawn into and culled from the shadow views (summed over views) and sprites culled (graphics Phase 12, see Frustum culling in [pipeline.md](pipeline.md)); batches (the scene pass's instanced draws, one per mesh and material); GPU bytes by kind: textures and meshes (counted at upload and when freed), render targets (`FrameTargets`, the shadow maps and the turned-text sheet, counted when made) and buffers; GPU particles; the LOD distribution (`lod_items`: items in view whose mesh has levels of detail, by the level drawn, one cross-fading counted at the level coming in; `LOD_LEVELS` is 3); surface shaders compiled and not compiling; and the GPU's time for the frame and for each pass (`pass_times`, `pass_time(FramePass)`).
- **Per-pass GPU time** (`renderer/gpu_timer.rs`) needs `Features::TIMESTAMP_QUERY`, which the editor and the benchmarks ask for where the adapter has it and the player never does. An empty compute pass writes a timestamp before each `FramePass` (Shadows, Depth prepass, Ambient occlusion, Volumetric fog, GPU particles, Scene, Post-processing, UI) and one after the last, so a part made of many render passes (three cascades, the bloom chain) is timed whole. A pass the frame didn't draw (shadows, ambient occlusion or volumetric fog off, no GPU particles, no UI) is `None`. The readings come back a frame or two later without the CPU waiting.
- **The editor's Profiler** (`editor/src/profiler.rs`) shows these while playing, in its **Graphics** tab (graphics Phase 37): **Frame** (CPU frame time with the peak, the viewport's CPU time to draw, GPU frame time), **GPU passes** (each pass's average time, "off" for one no recent frame drew, or one "not available on this GPU" line without timestamps), **Draws** (draw calls, batches, triangles, vertices), **Culling** (items drawn of submitted, items culled, shadow casters drawn and culled, sprites culled, GPU particles, lights), **Levels of detail** (Full detail, Level 1, Level 2), **GPU memory** by kind with a total, and **Shaders** (surface shaders compiled and not compiling, and the last 20 compile errors, which also go to the Output window). Times are averaged over the last 120 frames; counts are the newest frame's `RenderStats` as they are. Shader messages are kept across Plays, since each is reported once.

## Benchmarks

`player/src/bench_tests.rs` builds four scenes in code and draws each at 1280 × 720 on the High preset (MSAA 4×, three 2048² cascades, ambient occlusion High). They're ignored tests:

```text
cargo test -p axcenity-player --release -- --ignored bench_ --nocapture --test-threads 1
```

Each prints its last frame's counts (items drawn, batches, draw calls, triangles, and what was culled), then the times. Each warms up for at least a second (models load, caches fill, the GPU clocks up), then draws 5 batches of 60 frames, waiting for the GPU after each (untimed) so frames don't queue. Every number printed is the fastest batch's median, so a moment of other work on the machine doesn't count. CPU steps are `build_draw_list` (and, for the characters, `Animators::update`, which poses them) and `ScenePipeline::draw` (the CPU side of `TextureRenderer::render`: planning, uploads, recording and submitting).

| Benchmark | Scene |
|---|---|
| `bench_10000_parts` | 100 × 100 grid of blocks, balls, cylinders and wedges on a floor, drawn twice: an overview with all in view, and from one edge with most out of view (culling is measured against both) |
| `bench_2000_mesh_parts` | 2,000 copies of hello-pbr's textured helmet (about 15,500 triangles each, drawn in five views: three cascades, the prepass and the scene) |
| `bench_terrain_with_foliage` | A 2,048-stud terrain of 1,025² samples with hills, two layers and 20,000 plants (grass, flowers, bushes, trees) |
| `bench_20_animated_characters` | 20 copies of hello-animation's Noodle (about 200 triangles each), each with an Animator, posed every frame |
| `bench_20_walking_characters` | 20 copies of hello-character's walking character (about 76,000 triangles and 8 primitives each), each with an Animator, posed every frame (added in graphics Phase 14) |
| `bench_500_bullet_holes_in_the_graphics_reference_scene` | hello-graphics from (12, 8, 16), drawn without decals, then with 500 bullet holes spawned by `workspace:SpawnDecal` from a grid of raycasts (its cap raised to keep them all); fails if the GPU frame with them is over 16.7 ms (added in Phase 75, see [decals.md](decals.md)) |
| `bench_2000_mesh_parts_behind_a_wall` | The 2,000 helmets in view but hidden behind an 80-stud wall the camera stands 40 studs from, then the same view with no helmets (added in graphics Phase 15, for occlusion culling) |
| `bench_project` | A project of your own: set `AXCENITY_BENCH_PROJECT` to its folder. Drawn from its SpawnLocation at eye height looking ahead, then from an overview 120 studs up and 250 back. Skipped when the variable isn't set. Use it to find what a slow project costs before changing it or the engine (see Planning draws below) |

## Baseline

Taken at the commit that added the benchmarks, on an AMD Radeon RX 9070 XT (Vulkan, driver 26.10.1), Windows 11. Three runs agreed within 10% on every number (the widest was `build_draw_list` for the MeshParts, 8%; GPU times within 4%). Times are milliseconds. Absolute numbers depend on the machine; compare runs on one machine.

| | 10,000 parts, overview | 10,000 parts, edge | 2,000 MeshParts | Terrain + foliage | 20 characters |
|---|---|---|---|---|---|
| Items drawn / submitted | 10,001 / 10,001 | 10,001 / 10,001 | 2,001 / 2,001 | 37,691 / 37,691 | 41 / 41 |
| Batches | 4 | 4 | 2 | 1,027 | 41 |
| Draw calls | 20 | 20 | 10 | 5,135 | 205 |
| Triangles | 13.9 M | 13.9 M | 154.5 M | 53.7 M | 4,060 |
| CPU `Animators::update` | | | | | 0.05 |
| CPU `build_draw_list` | 3.28 | 3.33 | 0.64 | 6.48 | 0.01 |
| CPU `ScenePipeline::draw` | 2.44 | 2.43 | 0.92 | 12.14 | 0.28 |
| GPU frame | 1.92 | 2.31 | 9.29 | 5.99 | 0.43 |
| GPU shadows | 0.83 | 1.20 | 4.29 | 2.90 | 0.04 |
| GPU depth prepass | 0.27 | 0.26 | 1.24 | 0.95 | 0.02 |
| GPU ambient occlusion | 0.05 | 0.04 | 0.03 | 0.04 | 0.04 |
| GPU scene | 0.70 | 0.73 | 3.68 | 2.02 | 0.26 |
| GPU post-processing | 0.07 | 0.07 | 0.04 | 0.07 | 0.07 |
| GPU memory: textures / meshes / targets / buffers (MB) | 0 / 0 / 110.5 / 4.6 | same | 106.7 / 0.9 / 110.5 / 2.7 | 11.2 / 6.5 / 110.5 / 11.4 | 0 / 0.1 / 110.5 / 2.4 |

What it says, for the phases that follow:

- **Nothing is culled.** The edge view, with most parts out of view, draws every item and costs no less (its shadows cost more). Shadows are the largest GPU pass in every heavy scene (43% to 52% of the frame), since each cascade draws every caster. Frustum culling (Phase 12) should cut both.
- **Triangles dominate the MeshPart scene** (154.5 M a frame, 9.3 ms on the GPU, under 2 ms on the CPU): the case for LOD (Phase 13).
- **The terrain scene is CPU-bound**: 1,027 batches (mostly its 1,024 chunks, each its own mesh) drawn in five views make 5,135 draw calls, and `ScenePipeline::draw` takes 12 ms, twice the GPU's time. `build_draw_list` takes another 6.5 ms for its 37,691 items. Culling chunks and plants (Phase 12) and a foliage cull distance (Phase 13) should cut both.
- **CPU posing is not a cost** at 20 characters of this rig (0.05 ms), so GPU skinning (Phase 14) isn't called for by this benchmark. Each posed character is its own mesh and batch (41 batches for 41 items), which a heavier rig or many more characters would feel first.

## Frustum culling (graphics Phase 12)

Measured at the commit that added culling, against the commit before it, both on the same machine and session as above (AMD Radeon RX 9070 XT, Vulkan), three runs each, alternating. Each cell is the range of the three runs' numbers. The GPU's clocks differed from the baseline's day (the terrain's GPU frame took 9.1 ms before culling, not 6.0), so compare within this table.

| | 10,000 parts, overview | 10,000 parts, edge | 2,000 MeshParts | Terrain + foliage | 20 characters |
|---|---|---|---|---|---|
| Items drawn / submitted, before | 10,001 / 10,001 | 10,001 / 10,001 | 2,001 / 2,001 | 37,691 / 37,691 | 41 / 41 |
| Items drawn / submitted, after | 9,960 / 10,001 | 920 / 10,001 | 1,995 / 2,001 | 27,896 / 37,691 | 41 / 41 |
| Shadow casters drawn / culled (over all views), after | 7,555 / 22,448 | 3,618 / 26,385 | 2,060 / 3,943 | 2,040 / 111,033 | 123 / 0 |
| Draw calls, before → after | 20 → 14 | 20 → 20 | 10 → 9 | 5,135 → 1,682 | 205 → 205 |
| Triangles, before → after | 13.9 M → 7.6 M | 13.9 M → 1.5 M | 154.5 M → 93.4 M | 53.7 M → 16.6 M | 4,060 → 4,060 |
| GPU frame, before | 1.92–1.95 | 2.32–2.36 | 9.07–9.13 | 9.11–9.12 | 0.43 |
| GPU frame, after | 1.38–1.39 | 0.82 | 6.43–6.49 | 2.98–3.62 | 0.43 |
| GPU shadows, before → after | 0.84 → 0.29 | 1.22 → 0.19 | 4.19 → 1.52 | 3.98 → 0.24 | 0.04 → 0.04 |
| CPU `ScenePipeline::draw`, before | 2.38–2.55 | 2.38–2.55 | 0.95–0.97 | 12.47–13.55 | 0.27 |
| CPU `ScenePipeline::draw`, after | 2.97–3.46 | 2.20–2.52 | 0.83–0.90 | 12.96–13.22 | 0.27 |

What it says:

- **The edge view, with most parts out of view, now costs a third of the GPU time** (2.3 ms to 0.8 ms): 9,081 items are culled, and the cascades draw only the casters that reach them. Its CPU time is unchanged.
- **Shadows were the biggest win everywhere.** Each cascade used to draw every caster. Now it draws only those whose spheres reach its box, which already reaches 500 studs back toward the sun, so casters out of view that shade what's in view still draw.
- **The overview costs more CPU** (about 0.5 to 0.9 ms; `build_draw_list`, which this phase didn't change, also read about 10% higher in those runs, so part of it is noise). Every cascade copies its casters' instance data after the scene's (7,555 copies), and the instance upload grows from 4.6 to 6.9 MB. Planning was made cheaper to pay for most of it: each item's mesh and material keys are worked out once, and items are sorted as indexes instead of by recomputed keys. If a scene with many casters in every cascade shows this cost, drawing each cascade from the scene's instances where a whole batch is inside would avoid the copies.
- **The terrain scene is still CPU-bound** (13 ms in `ScenePipeline::draw`), even with a third of its draw calls. The time goes to planning and recording its 37,691 items. A foliage cull distance and LOD (Phase 13) are next.
- **Characters are unchanged**: all 41 items are in view.


## Levels of detail and foliage cull distance (graphics Phase 13)

Measured at the commit that added levels of detail, against the commit before it, on the same machine as above (AMD Radeon RX 9070 XT, Vulkan), three runs each, alternating. Each cell is the range of the three runs. Compare within this table.

| | 10,000 parts, overview | 10,000 parts, edge | 2,000 MeshParts | Terrain + foliage | 20 characters |
|---|---|---|---|---|---|
| Items drawn / submitted, before | 9,960 / 10,001 | 920 / 10,001 | 1,995 / 2,001 | 27,896 / 37,691 | 41 / 41 |
| Items drawn / submitted, after | 9,960 / 10,001 | 920 / 10,001 | 1,995 / 2,001 | 1,741 / 37,691 | 41 / 41 |
| Draw calls, before → after | 14 → 14 | 20 → 20 | 9 → 9 | 1,682 → 1,680 | 205 → 205 |
| Triangles, before → after | 7.6 M → 7.6 M | 1.5 M → 1.5 M | 93.4 M → 23.3 M | 16.6 M → 1.7 M | 4,060 → 4,060 |
| GPU frame, before | 1.39–1.41 | 0.82–0.83 | 6.42 | 2.97–4.47 | 0.43 |
| GPU frame, after | 1.45–1.48 | 0.84–0.85 | 4.39–4.49 | 1.28–1.29 | 0.43–0.44 |
| GPU scene, before → after | 0.71 → 0.75–0.77 | 0.46–0.47 → 0.47 | 3.61 → 2.45–2.50 | 1.73–3.04 → 0.91–0.92 | 0.26 → 0.26 |
| CPU `ScenePipeline::draw`, before | 3.01–3.74 | 2.20–2.51 | 0.86–0.94 | 13.66–14.64 | 0.27 |
| CPU `ScenePipeline::draw`, after | 3.15–3.66 | 2.37–2.81 | 0.95–0.96 | 3.66–3.75 | 0.28–0.29 |
| GPU meshes, before → after (MB) | 0 → 0 | 0 → 0 | 0.9 → 0.3 | 6.5 → 6.5 | 0.1 → 0.1 |

What it says:

- **The helmets draw a quarter of the triangles** (93.4 M to 23.3 M) and the GPU frame drops by a third (6.4 ms to 4.4 ms). Seen from 200 studs, every copy draws its second simpler copy (a quarter of the helmet's 15,500 triangles). Only the levels drawn are uploaded, so mesh memory drops too.
- **The terrain scene is no longer CPU-bound.** Plants past their cull distance aren't planned, uploaded or drawn: 35,950 of the 37,691 items are left out (the camera is 900 studs from the middle of a 2,048-stud terrain), so `ScenePipeline::draw` takes 3.7 ms, not 14, and the GPU frame 1.3 ms, not 3 to 4.5. `build_draw_list` (6.2 ms) still builds every plant; culling them in `scene` would save most of that.
- **Characters are unchanged**: the noodle's primitives are too small to simplify (under 256 triangles each).
- **Plain parts cost about 5% more in the scene pass** (0.71 ms to 0.76 ms for the overview): each instance is 4 bytes wider (the fade) and the fragment shader checks the dither. Nothing else changed for them.

## GPU skinning (graphics Phase 14)

The baseline's Noodle is too small to show what posing costs (0.05 ms). So this phase added `bench_20_walking_characters`, 20 of hello-character's walking characters (about 76,000 triangles each), and measured that first. Posing on the CPU was a real cost there: each frame, `ScenePipeline::draw` skinned every posed primitive (`Posed::primitive` runs when the renderer first reads the corners), checked its 4.5 million indices, rebuilt its corners and wrote 13 MB of them to the GPU, about 5 ms of a CPU frame whose GPU work took 0.8 ms. Every posed part was also its own mesh and batch.

Measured at the commit that skins on the GPU, against the commit before it with the new benchmark, on the same machine as above (AMD Radeon RX 9070 XT, Vulkan), three runs each. Each cell is the range of the three runs. Compare within this table.

| | 10,000 parts, overview | 10,000 parts, edge | 2,000 MeshParts | Terrain + foliage | 20 characters (Noodle) | 20 walking characters |
|---|---|---|---|---|---|---|
| Batches, before → after | 4 → 4 | 4 → 4 | 2 → 2 | unchanged | 41 → 3 | 149 → 22 |
| Draw calls, before → after | 14 → 14 | 20 → 20 | 9 → 9 | unchanged | 205 → 15 | 731 → 103 |
| CPU `ScenePipeline::draw`, before | 3.18–3.43 | 2.35–2.40 | 0.94–0.96 | 3.87–3.95 | 0.29 | 4.85 |
| CPU `ScenePipeline::draw`, after | 3.22–3.34 | 2.41–2.45 | 1.01–1.02 | 4.00–4.10 | 0.18 | 0.37–0.38 |
| GPU frame, before | 1.45 | 0.84 | 4.40–4.42 | 1.28–1.29 | 0.43–0.44 | 0.79–0.86 |
| GPU frame, after | 1.46–1.47 | 0.84–0.85 | 4.53–4.56 | 1.28–1.29 | 0.43–0.44 | 0.80–0.81 |
| GPU scene, before → after | 0.76 → 0.77 | 0.47 → 0.47 | 2.46 → 2.57–2.60 | 0.91 → 0.91 | 0.26 → 0.26 | 0.48–0.50 → 0.47–0.48 |
| GPU meshes, before → after (MB) | 0 → 0 | 0 → 0 | 0.3 → 0.3 | 6.5 → 6.5 | 0.1 → 0.1 | 13.0 → 2.3 |

What it says:

- **The walking characters cost a thirteenth of the CPU time** (4.85 ms to 0.38 ms in `ScenePipeline::draw`): nothing is skinned, checked or uploaded per frame but the joints (112 bytes each). The 20 characters share one copy of each primitive, so 149 batches become 22 and mesh memory drops from 13 MB to 2.3 MB. The GPU does the blending at no measurable cost.
- **Vertex-heavy scenes pay a little for it.** Every instance is 8 bytes wider (its skin) and every vertex checks it, so the helmets' scene pass, with 23 M triangles, takes 5% longer (2.46 ms to 2.59 ms) and their CPU time 6% (the wider instances are copied for every shadow view). Plain parts and terrain are within noise. If that matters, skinned items can get pipelines of their own, so other items skip the check.

## Texture budget and occlusion culling (graphics Phase 15)

**The texture memory budget** (see [shadows.md](shadows.md)) costs nothing while textures fit: each frame checks the total once. None of the benchmarks passes even the Low preset's 256 MB (the helmets take 106.7 MB), so their numbers are unchanged. Over the budget, a texture shrinks by a GPU copy of its smaller levels, once, and only grows again (an upload) when it fits.

**Occlusion culling was not built.** The plan allowed it only if a benchmark showed overdraw-heavy scenes that frustum culling and LOD don't fix. `bench_2000_mesh_parts_behind_a_wall` measures the most it could save: everything hidden items cost, as the hidden view minus the empty one. Three runs on the machine above (AMD Radeon RX 9070 XT, Vulkan) agreed within 0.1 ms; times are milliseconds.

| | Helmets hidden behind the wall | No helmets | Hidden items cost |
|---|---|---|---|
| Items drawn / submitted | 1,840 / 2,002 | 2 / 2 | |
| Triangles | 23.2 M | 120 | |
| CPU `build_draw_list` | 0.69–0.81 | 0.00 | 0.7–0.8 |
| CPU `ScenePipeline::draw` | 1.08–1.13 | 0.16–0.17 | 0.9 |
| GPU frame | 3.52–3.53 | 0.37–0.39 | 3.1 |
| GPU shadows | 1.29 | 0.03 | 1.26 |
| GPU depth prepass | 0.64–0.65 | 0.02 | 0.63 |
| GPU scene | 1.46 | 0.24–0.26 | 1.2 |

What it says:

- **The hidden helmets aren't overdraw.** The wall is drawn first (built-in shapes sort before meshes), so their pixels fail the depth test before shading. What they cost is vertex work for 23 M triangles that LOD has already cut to a quarter, and CPU planning.
- **Culling them by a Hi-Z pyramid would save at most 1.9 ms of GPU** (the prepass and scene), not the 1.3 ms of shadows: casters behind the wall still shade what the sun's cascades cover, and camera occlusion doesn't cull shadow casters. That is from a scene built to be its best case, 2,000 15,500-triangle models packed behind one wall, and its frame still takes 3.5 ms at 1280 × 720 on High. The other benchmarks have no large occluders, so it would save nothing there.
- **It would cost more than it saves today.** The prepass exists only with ambient occlusion on (Off at Low), so Low, the preset for weak GPUs, would need a depth pass of its own. Testing last frame's bounds on the CPU needs a readback a frame or two old, which culls an item for those frames as it comes out from behind something: the popping the criterion rules out, unless items are drawn GPU-driven (compute culling and indirect draws), a rewrite of `plan_draws` and its batching.
- **When to reconsider:** a real scene (an interior, a city) whose GPU scene and prepass time is mostly hidden items, measured the same way (with and without what's hidden). Cheaper fixes come first: a cull distance on dense models (as foliage has, `DrawItem::cull_distance`) and coarser LODs.

## GPU particles (graphics Phase 36)

`bench_particles` draws one emitter keeping 1,000 to 4,000 particles alive (`Rate × Lifetime`, Lifetime 2, a cloud in front of the camera), moved and sorted on the CPU and then on the GPU, then 100,000 on the GPU. CPU steps are `Particles::update`, `build_draw_list`, `Particles::draw` and `ScenePipeline::draw`. Measured on the machine above (AMD Radeon RX 9070 XT, Vulkan). The 2,000-on-the-GPU row was taken with the threshold lowered to 2,000. Times are milliseconds.

| Particles out | 2,000 CPU | 2,000 GPU | 3,000 CPU | 3,000 GPU | 4,000 CPU | 4,000 GPU | 100,000 GPU |
|---|---|---|---|---|---|---|---|
| CPU, all steps | 0.39 | 0.35 | 0.46 | 0.26–0.29 | 0.60 | 0.34 | 0.43 |
| GPU frame | 0.50 | 0.62 | 0.53 | 0.56–0.62 | 0.55 | 0.68 | 1.81 |
| GPU particles pass | | 0.12 | | 0.10–0.11 | | 0.13 | 0.23 |
| GPU scene | 0.32 | 0.32 | 0.34 | 0.30–0.33 | 0.37 | 0.36 | 1.48 |
| CPU + GPU | 0.89 | 0.97 | 0.99 | 0.82–0.92 | 1.15 | 1.02 | 2.24 |

What it says:

- **On the CPU, particles cost about 0.1 ms of CPU time per 1,000**, mostly building, sorting and uploading their sprites in `ScenePipeline::draw`. On the GPU they cost a nearly fixed 0.10 to 0.13 ms (the sort's dispatches, whatever the count up to its 4,096 slots) and almost no CPU time.
- **The GPU wins from about 3,000 particles**, so `scene::GPU_PARTICLE_THRESHOLD` is 3,000. At 2,000 the GPU path costs more GPU time than it saves on the CPU. Weaker GPUs pay more for the fixed cost, and weaker CPUs more per particle. The CPU's cap is 4,000 per emitter, so an emitter needing more than that has to be on the GPU anyway.
- **100,000 particles take 0.23 ms to move and sort** (131,072 slots), and 1.5 ms to draw: filling overlapping squares, not moving them, is the cost there. That's the same per pixel as the CPU path would pay.
- `RenderStats::gpu_particles` counts the slots kept, and their buffers (88 bytes a slot) count in the buffer bytes.

## Debug views (graphics Phase 38)

View > View Mode in the editor (`editor/src/viewport/view_mode.rs`) shows the viewport **Lit** (the game's frame), as one of the renderer's debug views, or as **Culling Bounds**: the lit frame with the culling overlay on (culling held where the camera was when it was picked, each item's bounds green where drawn and red where culled). The mode isn't saved between runs. The debug views (`render/src/renderer/debug_views.rs`, `debug_views.wgsl`, `TextureRenderer::set_debug_view`):

- **Unlit**: each surface's base color plus its glow. **Base Color**: the base color alone, its color map included. Both are as the color is written (sRGB), with no light, shadow, fog or post-processing.
- **Wireframe**: each triangle's edges, one pixel wide, over its surface dimmed. The edges come from each corner's place in its triangle, worked out in the shader (line drawing isn't on every backend).
- **Normals**: the way each surface faces in the world, normal maps included, x, y and z as red, green and blue from -1 (0) to 1 (255).
- **Metalness**, **Roughness**: the value, maps included, as grey from 0 (black) to 1 (white).
- **Ambient Occlusion**: the occlusion map times screen-space ambient occlusion (solid items only, and only while the Ambient Occlusion graphics setting is on), white for open.
- **Overdraw**: every surface drawn over a pixel adds the same orange light, hidden ones included, with back faces culled as the scene culls them.
- **LOD Levels**: green for an item drawn with its own mesh, then yellow, orange and red for its simpler copies (graphics Phase 13); grey for a mesh without levels.

A debug view draws after the frame's own passes, over the finished picture and under the UI, at the output's size, with its own depth target and the camera's view (without TAA's jitter). Each batch is drawn again with its material, posed and swayed as in the scene, reading its corners from the mesh's buffers ("vertex pulling"). See-through items draw as solid. Terrain chunks show their shape and normals with the plain white material, and a material's own surface shader is left out: its maps and values show as the standard material reads them. Sprites and the sky aren't drawn.

The debug views are built only with `render`'s `debug-views` feature, which the editor turns on, and the player too since Phase 71 for its Developer Console's `wireframe` (`Renderer::set_debug_view`). With the feature, mesh buffers can also be read as storage buffers. While the lit view is shown, nothing is kept for the debug views: their pipelines, depth target and uniforms are freed when the mode goes back to Lit.

## Decals (Phase 75)

`bench_500_bullet_holes_in_the_graphics_reference_scene`, on the machine above (AMD Radeon RX 9070 XT, Vulkan), at 1280 × 720 on High. hello-graphics already keeps the prepass at High (ambient occlusion), so the holes add only their own pass. Times are milliseconds.

| | Without decals | 500 bullet holes |
|---|---|---|
| Decals drawn | 0 | 500 (one draw call) |
| GPU frame | 0.640 | 0.734 |
| GPU scene pass (decals are drawn in it) | 0.416 | 0.515 |
| GPU depth prepass | 0.028 | 0.028 |

So 500 projected decals cost about 0.1 ms of GPU time, well inside a 60 frames a second budget (16.7 ms). Games keep at most `MaxSpawnedDecals` (200 by default) from `workspace:SpawnDecal`, and the quality preset draws at most 512 at High.

## A dense painted terrain (`bench_project`)

The first project measured with `bench_project` was a 960 × 1,260-stud terrain with 8,339 models painted as foliage (grass, ferns, pebbles, bushes and 10-stud clumps of trees from a model pack, 20 to 550 triangles each) and 600 MeshParts (trees, boulders and a store), on the machine above (AMD Radeon RX 9070 XT, Vulkan), release build, 1280 × 720 on High. Times are milliseconds; the GPU frame was 2.8 to 3.0 ms throughout, so every number that mattered was CPU time.

| | At the spawn, before | At the spawn, after | Overview, before | Overview, after |
|---|---|---|---|---|
| Items submitted / drawn | 22,368 / 11,897 | 16,035 / 10,339 | 22,368 / 19,591 | 12,091 / 10,708 |
| CPU `build_draw_list` | 7.7 | 5.7 | 7.7 | 4.3 |
| CPU `ScenePipeline::draw` | 19.5 | 12.5 | 19.3 | 8.0 |
| GPU frame | 2.9 | 2.9 | 3.0 | 2.8 |

What it said, and what changed:

- **Painted models had no cull distance.** Built-in plants are left out past a distance by their kind, but a model painted from the Assets panel was drawn everywhere, so 8,339 of them became 17,600 draw items a frame across the whole terrain, ferns a third of a stud tall included. Now a painted model's cull distance comes from its size as placed, 120 studs per stud of its largest extent from 60 to 1,500 (`scene::foliage::model_cull_distance`, the built-in plants' rate), and `scene` skips a plant or model past it before building its items, looking each model up once per frame rather than once per instance (`docs/terrain.md`).
- **Planning cost 11 to 15 ms of the 19.5**, timed inside `plan_draws`: 2 to 3 ms working out each draw's `MaterialKey` (330 bytes of texture keys and map settings), 2 to 5 ms sorting 22,000 indexes by comparing those keys and re-deriving each item's shader key per comparison, and 6 to 7 ms in the loop, where `instance_raw` converted six sRGB channels with `powf` per item. `upload_new` worked out every `MaterialKey` a second time. Now `ScenePipeline::draw` works the keys out once per draw and hands them to both; `plan_draws` sorts by a small `SortKey` (mesh, the material's index among the frame's distinct keys, with the plain material at index 0 without a lookup, shader, sidedness, shadow) and batches by it; and `LinearColors` keeps the last base and glow colors converted, since a batch's items share them.
- **What's left is the count.** 16,000 items still reach the renderer from the spawn, which stands in a forest: clumps of trees are drawn to 1,200 studs, as their size says, and each is three to nine draw items. The next savings there are fewer primitives per painted model (one material per clump) and coarser levels of detail, which are the project's to make, or combining a terrain's painted instances of one model into one draw item each (the renderer already batches them into one instanced draw).
- **The texture budget was full for another reason.** The project's `realistic/` models embed 2048² textures (21) and three 4096² ones, 540 MB with mips of the 1,012 MB on the GPU, so High's 1,024 MB budget had shrunk one. Nothing in the frame times came from that (the budget check costs 0.2 ms and nothing was replaced), but it's the first thing to downsize in such a pack.
- **Debug builds pay more.** `scene`, `physics` and `terrain` are among the crates `[profile.dev.package]` optimizes, since the debug editor spends each frame in them (`build_draw_list` 13.4 ms unoptimized, 6.8 ms optimized, on this project). The engine's other crates stay unoptimized, so measure in release before blaming a project.

### Play and the selection box on that terrain (`bench_project_play`)

Selecting something made the editor crawl and Play was a slideshow, while neither the CPU nor the GPU looked busy: the work was on one thread (1 of 32, about 3% of the CPU in Task Manager) and the GPU had under 3 ms of work a frame. `bench_project_play` in `editor/src/play/tests.rs` times the game frame without drawing, and the selection box, on the project `AXCENITY_BENCH_PROJECT` names:

```text
cargo test -p axcenity-editor --release -- --ignored bench_project_play --nocapture --test-threads 1
```

| Release build | Before | After |
|---|---|---|
| `Session::frame`, walking (median) | 666 ms | 2.0 ms |
| Selection box, per selected part | 17 ms | 0.1 to 0.3 ms |

In a debug build the same frame is 8.5 ms and the box 0.5 to 1.4 ms.

- **Every physics query built every collider.** `candidates` (`physics/src/query.rs`) made a collider for each part in the Workspace per ray: a convex hull per MeshPart, and a copy of all 345,249 height samples for the terrain. The walking character casts many rays a frame. Now a `Reach` (the capsule the query can touch) passes by parts whose bounding ball is outside it before anything is built, and a Terrain's colliders are kept between queries by its `uid` and `revision` (`terrain_colliders`; a copy of a terrain has its own `uid`, and a reloaded model rebuilds them).
- **The selection box walked the world per ray.** Up to 12 edges of 17 rays each, every ray listing the Workspace twice and marching across the whole terrain. Now `scene::Occluders` gathers the parts and terrains once a frame, each edge tests only the parts near the line of sight to it, and terrain rays stop at the edge (`terrain::raycast::raycast_within`).

## A painted forest (`shoot_project`)

A 600-stud forest with 30,296 painted models (a project called Abandoned Farmhouse, all models in `.glb` packs with 4K embedded textures) was a slideshow: each of its 36,000 items was listed again and walked through the renderer every frame though only 5,700 were on screen. Pictures and timings come from `shoot_project` (`player/src/bench_tests.rs`), which draws named cameras and saves a PNG of each; `bake_project_probes`, beside it, bakes a project's light probe volumes without the editor:

```text
AXCENITY_BENCH_PROJECT=<folder> AXCENITY_SHOT_DIR=<folder> AXCENITY_SHOT_VIEWS='front:2.7,4.3,63:-0.6,11.5,0' \
  cargo test -p axcenity-player --release -- --ignored shoot_project --nocapture --test-threads 1
```

`AXCENITY_SHOT_SIZE` and `AXCENITY_SHOT_QUALITY` choose the frame, and `AXCENITY_SHOT_DEBUG` (`ao`, `base`, `normals`, `overdraw`) saves a debug view.

| 687 × 1024, High, release | Before | After |
|---|---|---|
| CPU `build_draw_list` | 12.2 ms | 1.2 ms |
| CPU `ScenePipeline::draw` | 29.8 ms | 13.3 ms |
| GPU frame (translucent leaves and probe light cost about 2.6 ms; the bands below save more at larger sizes) | 6.6 ms | 7.4 ms |

At 2560 × 1440 on Ultra, the GPU's frame was 40.1 ms before distance bands and is 29.0 ms now, and the CPU is 36 ms (it draws twice the items, and a fourth shadow cascade).

- **`DrawList::chunks` (`ItemChunk`)**: `scene` builds a terrain's foliage once (`foliage::PaintedFoliage`), in 48-stud squares with short-reach plants apart from trees, and hands the list `Arc`s of them. It's rebuilt only when the terrain or a painted model changes.
- **`Reach` (`renderer/culling.rs`)**: every view the frame draws (the camera, the sun's cascades and the lamps' shadow views). `LodStates::resolve` passes over a group that touches none of them, or that's past its farthest cull distance, before any per-item work; a kept group's items are checked one by one. A frozen view keeps everything.
- **`upload_new`** looks up each mesh and each set of textures once per frame, by address, rather than once per draw.
- **Real-time GI in the editor.** The editor was at 2 frames a second with Ultra and `realtime_gi` on: 434 of each 474 ms went to tracing probes (12 whole draws of a 36,000-item scene each frame, on one CPU thread, at 16² so the GPU had little to do). `light_state` saw the sky picture finish loading as the light changing, so all 1,472 probes were traced after every open, about six minutes. See Real-time GI in [sky.md](sky.md) for what a heavy scene does now.
- **Distance bands (`SortKey::band`)**: solid items are drawn nearest first in about 2 bands per doubling of the distance, then by mesh and material. Looking along a forest, every pixel had 8 or more layers of leaves, each shaded; a band drawn first hides what's behind it, by depth, before the shader runs. It's a few more batches for about a quarter less GPU time there.

## Sharing the frame's planning among the CPU's threads

That forest ran at 26 frames a second at 2560 × 1440 on Ultra: 40.8 ms of CPU on one thread of 32, and 31 ms of GPU. Measured with `shoot_project` from the same camera, on the machine above (AMD Radeon RX 9070 XT, Vulkan), release build. The picture is the same to within noise: 176 of 3.7 million pixels differ by more than 32 of 255. Times are milliseconds.

| 2560 × 1440, Ultra | Before | After |
|---|---|---|
| CPU `build_draw_list` | 1.2 | 1.3 |
| CPU `ScenePipeline::draw` | 39.6 | 9.5 to 10.0 |
| GPU frame | 31.0 | 13.6 to 13.9 |
| GPU shadows | 7.5 | 5.6 to 5.7 |
| GPU scene | 22.4 | 7.0 to 7.7 |

Inside `ScenePipeline::draw`, before: `LodStates::resolve` 5.2, material keys 3.4, `upload_new` 8.4, `plan_draws` 12.9, the shadow views' casters 4.1. Recording every draw took only 1.2, so it isn't threaded.

- **Threads (`renderer/parallel.rs`, rayon).** Levels of detail, material keys, sort keys, the sort, instance data and each shadow view's casters are worked out in shares of 512 items and joined in order, so a frame is the same however it was shared (`planning_comes_out_the_same_however_many_threads_share_it`). A terrain's groups outside every view are passed over before anything is shared. Each LOD state has its own lock, and states of items new this frame are added afterwards. Joining doesn't copy: `DrawPlan::instances` and `casters` stay in pieces, and `write_pieces` copies them straight into wgpu's staging memory. Under `WORTH_SHARING` (4,096) items a job stays on the calling thread: waking the others takes about 0.1 ms, which made small scenes slower. Past 16 threads there's no gain.
- **Work per distinct thing, not per draw (`FrameUses`).** Hashing a 330-byte `MaterialKey` for every draw, twice, was most of `upload_new` and much of `plan_draws`. Now each material and mesh is looked up once. `MeshSource::lod` hands out a kept copy instead of allocating one per draw per frame.
- **Depth fill for cut-out surfaces (`PrefillPipelines`).** A shader with `discard` can't have the GPU skip what's hidden, so every leaf in front of a pixel was fully lit, then covered. Solid standard-material batches with an `alpha_cutoff` now draw their depth first in the scene pass (`fs_depth_only`, which only reads alpha), then are lit with no `discard` where depth is equal (`fs_prefilled`), so each pixel is lit once. Surface shaders and terrain are drawn as before, since they decide their own holes. `fs_depth_only` must cut out exactly what `standard_surface` does.
- **Far cascades take turns (`ScenePipeline::schedule_cascades`).** Past `HEAVY_SHADOW_CASTERS` (12,000) casters planned, the two farthest cascades are drawn on alternate frames, keeping the matrix they were drawn with. They held 11.3 of the 14.3 million shadow triangles. A cascade is drawn anyway once the camera has moved 256 of its texels from where it was kept.
- **Light through a leaf (`soft_sun_visibility`)** takes four filtered shadow samples, not the full 32-sample soft shadow it was computing a second time per leaf pixel (0.6 ms).

Against the commit before, small scenes are unchanged or better (10,000 parts: 3.4 to 4.0 down to 2.9; 2,000 helmets: 1.8 down to 1.2; terrain and foliage: 3.1 to 3.7 down to 2.8; two items: 0.26 both). Filtering small casters out of the far cascades was tried and removed: it left out 2 of 21,000.

**What's left is GPU work**: at Ultra, 5.7 ms of shadow vertices and 7 ms of lighting at 3.7 million pixels. High at 2560 × 1440 is 9.4 ms of GPU. Each shadow view copies its casters' 176-byte instances, of which a shadow pass reads 88, so a slimmer instance would halve that upload.
