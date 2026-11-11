# Rendering performance

How drawing is measured, and the baseline that optimizations (culling, LOD, GPU skinning, occlusion) are compared against. Measure before optimizing: a change that claims to make drawing faster shows it here.

## What's measured

- **`RenderStats`** (`TextureRenderer::stats`, `render/src/renderer.rs`), for the last frame: draw calls, triangles and vertices (the drawn meshes' corners, every instance counted; shadow views and the prepass included); items submitted (in the draw list), drawn (by the scene pass) and culled (outside the camera's view); shadow casters drawn into and culled from the shadow views (summed over views) and sprites culled (graphics Phase 12, see Frustum culling in [pipeline.md](pipeline.md)); batches (the scene pass's instanced draws, one per mesh and material); GPU bytes by kind: textures and meshes (counted at upload and when freed), render targets (`FrameTargets`, the shadow maps and the turned-text sheet, counted when made) and buffers; GPU particles; the LOD distribution (`lod_items`: items in view whose mesh has levels of detail, by the level drawn, one cross-fading counted at the level coming in; `LOD_LEVELS` is 3); surface shaders compiled and not compiling; and the GPU's time for the frame and for each pass (`pass_times`, `pass_time(FramePass)`).
- **Per-pass GPU time** (`renderer/gpu_timer.rs`) needs `Features::TIMESTAMP_QUERY`, which the editor, the benchmarks and the player ask for where the adapter has it. The player times frames only while `Renderer::set_gpu_timing` is on: during `--bench`, and while its `fps` readout or the console's Stats tab is shown, so a game nobody is measuring pays nothing. `RenderStats::gpu_readings` counts the readings, so a reader that keeps every frame's time takes a reading once. An empty compute pass writes a timestamp before each `FramePass` (Shadows, Depth prepass, Ambient occlusion, Volumetric fog, GPU particles, Scene, Post-processing, UI) and one after the last, so a part made of many render passes (three cascades, the bloom chain) is timed whole. A pass the frame didn't draw (shadows, ambient occlusion or volumetric fog off, no GPU particles, no UI) is `None`. Inside the Shadows pass each of the sun's cascades is stamped too (`cascade_times`, with `cascade_casters` and `cascade_triangles`); passes with no dependency between them can overlap on the GPU, so treat a cascade's time as approximate. The readings come back a frame or two later without the CPU waiting.
- **The editor's Profiler** (`editor/src/profiler.rs`) shows these while playing, in its **Graphics** tab (graphics Phase 37): **Frame** (CPU frame time with the peak, the viewport's CPU time to draw, GPU frame time), **GPU passes** (each pass's average time, "off" for one no recent frame drew, or one "not available on this GPU" line without timestamps), **Draws** (draw calls, batches, triangles, vertices), **Culling** (items drawn of submitted, items culled, shadow casters drawn and culled, sprites culled, GPU particles, lights), **Levels of detail** (Full detail, Level 1, Level 2), **GPU memory** by kind with a total, and **Shaders** (surface shaders compiled and not compiling, and the last 20 compile errors, which also go to the Output window). Times are averaged over the last 120 frames; counts are the newest frame's `RenderStats` as they are. Shader messages are kept across Plays, since each is reported once. The Game tab adds the **1% low** over those frames, and the Graphics tab **Limited by** (GPU-bound, CPU-bound or sync-bound, `core::bench::Bound`, where sync-bound in the editor usually means its own UI or the display).
- **The `fps` readout** (player and editor Play, [developer-console.md](../developer-console.md)): `fps` shows frame rate, frame time, 1% low, CPU and GPU times and the bound in one line; `fps full` adds each frame step and the drawing's numbers (`axcenity_ui::render_lines`). Its cost wasn't measured on its own: the readout is a few UI quads and up to a dozen lines of text, and GPU timing is nine empty compute passes and a 72-byte readback every other frame, against frames of several milliseconds.

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
| `bench_build_2000_mesh_parts` (in `scene/tests/bench_mesh_parts.rs`, no GPU) | The same 2,000 helmets, timing only `build_draw_list_posed` with a `TerrainCache` kept across frames, so it runs on machines with no graphics adapter: `cargo test -p axcenity-scene --release --test bench_mesh_parts -- --ignored --nocapture` (added with I8) |
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

## Where the frame goes (`profile_project`)

`profile_project` (`player/src/profile_tests.rs`) measures a project from named cameras by leaving one cost out at a time and timing the frame again. Each variant changes only that run's draw list or graphics settings:

```text
AXCENITY_BENCH_PROJECT=<absolute folder> AXCENITY_SHOT_VIEWS='walker:-39.6,5.5,39.6:22,14,12' \
  AXCENITY_SHOT_SIZE=2560x1440 AXCENITY_SHOT_QUALITY=Ultra AXCENITY_SHOT_DIR=<folder for PNGs> \
  cargo test -p axcenity-player --release -- --ignored profile_project --nocapture --test-threads 1
```

| Variant | Leaves out |
|---|---|
| `no-shadows` | every shadow map (and the scene pass's shadow lookups) |
| `no-foliage-shadows` | shadows of painted foliage (items with a cull distance) |
| `no-wind`, `no-sway` | the wind (nothing bends), or every item's sway (the vertex shader skips it) |
| `no-cutout` | alpha testing: cut-out materials drawn solid (a different picture; it shows what the depth fill saves, not a saving to take) |
| `single-sided` | back faces of double-sided materials |
| `no-foliage` | painted foliage altogether |
| `no-large-meshes` | meshes of more than 100,000 triangles |
| `no-msaa`, `no-ao` | anti-aliasing, ambient occlusion |

Each variant's list is built once and drawn again and again with three frames in flight, so the GPU is never idle between frames: drawing a new list each frame left the GPU waiting on the CPU, it clocked down, and its timestamps read up to 70% slower (a first version of the tool had removals "costing" 3 to 4 ms). Variants are measured in three rounds through all of them, and each cell is the median of the rounds' medians. `AXCENITY_PROFILE_VARIANTS` picks some. After the table it reads the overdraw view back (`OverdrawStats`, below), and `AXCENITY_SHOT_DIR` saves a PNG of every variant to compare by eye.

`lod_report_project`, beside it, lists every model a project's MeshParts show or its terrains have painted with its levels of detail and their warnings (`assets::LodReport`), biggest first.

## Overdraw, as numbers (`OverdrawStats`)

`render::OverdrawStats::from_pixels` reads the overdraw view's picture back into numbers: layers per pixel over the picture and over covered pixels, the most on one pixel, the share of pixels in bands (none, 1, 2–3, 4–7, 8–15, 16–31, 32 and up, `OVERDRAW_BANDS`) and the busiest cells of an 8 × 6 grid over the screen. Each layer adds 0.02 to blue in linear light, so counts read back exactly up to `MAX_LAYERS` (50) from an 8-bit sRGB picture; 50 means "50 or more" (reported as `50+`).

**What it counts is depth complexity, not shading work.** The view draws every batch again with no depth test, back faces culled as the scene culls them, cut-out holes and dithered fades left out. So a pixel's count is how many surfaces the camera looks through there. The lit scene doesn't shade each of them: cut-out materials draw their depth first and are then lit once per pixel (`PrefillPipelines`), and an opaque surface drawn in front stops what's behind before shading (distance bands sort near first). The count is the *input* to the depth tests: the cost it predicts is vertex work and the cheap depth fill (`fs_depth_only` samples one alpha texel per layer), not full lighting per layer. Shadow maps, the prepass, MSAA samples, terrain's own passes, sprites and the sky aren't counted, and see-through items count as solid. The editor's overdraw view is the same picture; its brightest orange is 8 or more layers (red is full at 8.3 layers, green at 20, blue at 50).

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

## MeshParts share their meshes between frames (I8)

Each MeshPart used to make a new mesh for each primitive of its model every frame, with new textures for the file's material and (since each mesh kept its own) new simpler copies for its levels of detail. 2,000 copies of one helmet handed the renderer 2,000 meshes, which it told apart by address. Now `scene/src/mesh_cache.rs` keeps, in the host's `TerrainCache` (`TerrainCache::meshes`):

- **Each model's pieces, by `MeshId`**: one mesh and one file material per primitive, made once and shared by every MeshPart showing that model. The assets are asked for the model once a frame per `MeshId`; when they hand back a different one (`Assets::reload` swapped it in), its pieces are made again. A model nothing drew this frame is let go.
- **Each animated MeshPart's `Skin`**, kept while its `Posed::version` is the same. A new pose is copied into the same buffers when nothing else holds them (`Skin::set_pose`), using the normal matrices the pose already has (`Posed::normal_matrices`) instead of inverting every joint again.

`build_draw_list` (no cache passed) still makes them each call. `bench_build_2000_mesh_parts` on the cloud container (4 vCPU Xeon at 2.1 GHz, release build, fastest batch's median of 5 × 60 frames, four runs each):

| | Before | After |
|---|---|---|
| CPU `build_draw_list`, 2,000 helmets | 1.84 to 1.90 | 0.90 to 0.99 |

The player's `bench_2000_mesh_parts` keeps a `TerrainCache` across frames too now, as the hosts do; it wasn't run for this change (the container has only a software GPU).

## Retained draw items (M4)

Every frame used to walk the Workspace and work out every part's `DrawItem` from its properties, so the draw list cost grew with the world, not with what changed. Now the host's `TerrainCache` keeps a `DrawCache` (`scene/src/retained.rs`) holding each Instance's items between frames, and an Instance's items are made again only when something they were made from changes:

- **The Instance itself**: its `revision`.
- **What it reads from other Instances**: its SurfaceAppearance and light children, a Decal's part, and the Inventory whose `Equipped` puts a carried Tool in or out of the world. Their revisions are kept with the items. No other ancestor property is read while drawing, so editing a Model or Folder invalidates nothing.
- **The tree**: a move in or out of the Workspace, or among a parent's children (`World::child_order_version`), lists the entries again in tree order and rechecks each one's children and character membership; the rest keep their items. A frame whose tree didn't change doesn't walk it.
- **Everything at once**: another World (`World::identity`; a copy keeps its revisions), another or changed `Assets` (`Assets::generation`: loads, hot reloads, material edits, `forget`, the blueprint image) or a change in the bloom glow boost.

Lights, reflection and light probes, ProjectedDecals, joints and animated MeshParts are still worked out each frame, and streaming's hidden set is checked each frame. Terrain, UI and particles are drawn as before. The output is the same `DrawList`; `render` doesn't change.

**It's on by default.** `AXCENITY_RETAINED_DRAW_LIST=off` (or `0`) builds every frame from scratch instead, and `TerrainCache::set_retained_draw_list` does it from code. One-off `build_draw_list` calls (no cache passed) always build from scratch. `scene/src/retained_tests.rs` checks, over randomized worlds and edits, that each retained frame equals a from-scratch build item by item, plus one test per way a kept item could go stale.

Cloud container (4 vCPU Xeon at 2.1 GHz, release build), times in milliseconds. The CPU benches are the fastest batch's median of 5 × 60 frames over three runs each, alternating off and on. S10k is the player's `--bench 300` under Xvfb on lavapipe, two runs each, alternating; its Render and frame numbers are software rasterization, so only the Draw list step says anything about a real machine.

| | From scratch (off) | Retained (on) |
|---|---|---|
| `bench_build_10000_parts`, all still | 6.4 to 8.0 | 1.2 to 1.5 |
| `bench_build_10000_parts`, 1% moving each frame | 6.1 to 7.1 | 1.42 to 1.45 |
| `bench_build_2000_mesh_parts` | 0.94 to 0.99 | 0.36 |
| S10k player, Draw list median | 12.9 | 4.3 to 4.4 |
| S10k player, Draw list 95th percentile | 18.0 | 5.6 to 6.1 |
| S10k player, frame median (lavapipe) | 128.9 to 129.5 | 118.9 to 120.1 |

- **What's left when nothing changes is copying**: each frame still copies every kept `DrawItem` into the new list (a few hundred bytes and some `Arc` counts each), about 1.2 ms for 10,000 parts. S10k rebuilds about 1,100 Instances a frame (its falling drops, spinners and recolored blocks) on top of that.
- **Next steps** (report section 5.2, not done yet): put still, anchored parts into spatial `ItemChunk`s so whole cells skip the copy and the renderer's LOD resolve, then remove the from-scratch path.

## Small dense patches (biome test projects)

Nine 72 × 72-stud projects, about 35,000 painted models each (pebbles, litter, grass tufts) and two trees of 0.7 to 1 million triangles. Measured with `shoot_project` at 2560 × 1440, on the machine above (AMD Radeon RX 9070 XT, Vulkan), release build. The cells are the range over five of the projects and two cameras. Times are milliseconds; two runs of one view can differ by about 1.5 ms of GPU.

| Ultra | Before | After |
|---|---|---|
| Shadow casters drawn (all views) | 66,800 to 96,500 | 38,300 to 66,600 |
| CPU `ScenePipeline::draw` | 9.3 to 10.4 | 8.0 to 8.8 |
| GPU frame | 14.3 to 20.1 | 14.4 to 19.6 |

At High, after these changes, the GPU frame is 10.0 to 15.9.

- **Shadow reach (`SHADOW_REACH`, `renderer/lod.rs`).** An item with a cull distance casts only to 35% of it. Most of the casters were pebbles and tufts drawn into every cascade. The pictures from both cameras are the same to the eye.
- **Cut-out edges (`fs_depth_coverage`).** Alpha to coverage on the depth fill (see Anti-aliasing in [shadows.md](shadows.md)) costs up to about 1 ms of GPU here, which is about what the shadow reach saves, so the GPU frame is unchanged while the edges are smooth.
- **Simpler meshes for shadows were tried and removed.** Drawing shadows two levels of detail down saved about 1 ms, added 36 to 54 MB of mesh memory and visibly changed how the trees shaded themselves and the ground.
- **Game logic isn't the cost**: `Session::frame` is 0.4 ms in `bench_project_play`. What's left is triangles. The two trees are 1.7 million triangles in up to seven passes, about half of the frame's, so a project's first saving is lighter tree models.

## Temperate Dry Patch at Ultra, 2560 × 1440 (2026-11-11)

The owner's primary benchmark: a 72 × 72-stud biome patch (see Small dense patches above) on the desktop above (Ryzen 9 9950X3D, RX 9070 XT, Vulkan), release builds, pinned to the V-cache CCD.

**The player** (`axcenity-bench-check`'s `dry-patch-ultra-1440p`: `--size 2560x1440 --quality Ultra`, vertical sync off, 400 frames from the project's demo camera): **frame median 11.3–11.5 ms (about 88 FPS), 1% low 72–75 FPS, GPU-bound**: the GPU's frame is 11.3 ms of it. Game logic is under 0.3 ms (Draw list 0.1 ms); the CPU's drawing work (`Render`) is 4.8 ms and the rest is Present wait. GPU passes: shadows 3.8 ms, depth prepass 1.6, ambient occlusion 0.2, scene 5.6, post-processing 0.1. So the 20–30% CPU and 70% GPU utilization the owner saw with vertical sync on is a GPU-bound frame waiting on the display, not idle headroom on the CPU's side.

A first run reported a frame-time standard deviation of 2.6 ms and a mean 1.5 ms below the median. The timeline (`AXCENITY_BENCH_TIMELINE`) showed one step, not jitter: the project's `views.client.lua` holds a demo camera for 8 seconds and then hands the camera to the walker, whose view costs half as much. The check now measures 400 frames, all on the demo camera.

**Where it goes** (`profile_project`, three cameras: the walker's view, under the canopy, along the ground; the offscreen renderer at the same size and preset, so 1 to 2 ms under the player's frame). GPU frame in milliseconds, then the change from leaving each cost out:

| | Walker | Canopy | Ground |
|---|---|---|---|
| GPU frame (shadows / prepass / scene) | 9.09 (3.01 / 1.02 / 4.88) | 9.18 (3.59 / 1.07 / 4.26) | 9.13 (3.46 / 1.37 / 4.03) |
| No shadows at all | −4.52 | −4.99 | −4.40 |
| No shadows from painted foliage | (−0.9 shadows pass) | −0.96 | −1.12 |
| The two oaks left out (1.7 M triangles) | −3.84 | −4.50 | −2.56 |
| Painted foliage left out | −2.26 | −1.71 | −2.86 |
| Anti-aliasing off (MSAA 4×) | −1.46 | −0.74 | −0.77 |
| Double-sided drawn one-sided | −1.28 | −0.71 | −0.89 |
| Ambient occlusion off | −0.13 | −0.11 | −0.23 |
| No wind / no sway | −0.09 / −0.14 | −0.08 / −0.08 | −0.05 / −0.01 |
| Cut-outs drawn solid | +3.06 | +4.84 | +2.49 |

Overdraw (`OverdrawStats`, layers each pixel looks through): walker 16.8 per covered pixel (13.6% of the picture at 32 or more), canopy 14.6, ground 22.5 (24.5% at 32 or more, the busiest cells along the grass at the horizon, 44 to 49 layers).

What it says:

- **It's triangles and shadows, not fragments.** The two oaks are 969k and 738k triangles at LOD0 (`lod_report_project`), drawn in up to six views (cascades, prepass, depth fill, lit). Leaving them out saves 2.6 to 4.5 ms; leaving out all 35,000 painted ground models saves 1.7 to 2.9 ms. **Most of each oak is bark**: 549k and 556k bark triangles against 163k and 182k of leaves (plus 258k of moss cards on one), and the bark material is double-sided.
- **The bright overdraw is mostly cheap.** Cut-outs are lit once per pixel already (depth fill, then lit where depth is equal): drawing the same leaves as solid cards costs 2.5 to 4.8 ms *more*, which is what the depth fill saves. What a layer still costs is the depth fill's one alpha sample and its vertices. Overdraw is highest along the grass at grazing angles, where nothing can be culled behind it.
- **Wind is free.** Every variant without it is within 0.15 ms: the sway is a few vertex instructions on the GPU, with one shared `GlobalWind` in the frame uniform and no CPU work per plant (see [wind.md](wind.md)). Nothing to optimize.
- **Ambient occlusion is cheap** (0.1–0.2 ms). Anti-aliasing costs 0.7–1.5 ms and double-sided materials 0.7–1.3 ms, but turning either off changes the picture (jagged leaf edges, missing leaf backs), so neither is a saving to take at Ultra.
- **Shadows are the largest single pass** (3.0–3.6 ms; 4.4–5.0 ms with the lookups). Painted foliage's share is about 1 ms, even though `SHADOW_REACH` already limits it to 35% of its cull distance.

### Whose overdraw it is

`profile_project` with `AXCENITY_PROFILE_OVERDRAW_GROUPS=<n>` draws the overdraw view again with only one model primitive's items at a time (named `file: material` from the models the world's MeshParts and terrains show) and lists the `n` biggest by their share of all the layers. Temperate Dry Patch, Ultra 2560 × 1440:

| Share of the layers | Demo camera | Along the ground |
|---|---|---|
| The five grass models (13,180 tufts, 28–108 triangles each) | 73.8% | 80.4% |
| Both oaks' leaves | 5.2% | 10.4% |
| Oak bark | under 1% | 2.4% |
| Shrubs, rocks, thistles and the rest | the remainder | the remainder |

So the bright bands in the overdraw view are grass seen at a low angle, 6 to 12 layers where each grass model covers anything and 40 to 50 where they all overlap along the horizon. The canopy is a small share. What a grass layer costs is its corners and the depth tests (the alpha sample of the depth fill and the prepass), not lighting; leaving all painted foliage out saved 1.7–2.9 ms of GPU, a third of it their shadows. Fewer layers there means fewer tufts far away (density by distance), which changes how full the ground looks; a choice for the project or the preset, not a fix.

### Thinning far ground cover (a graphics setting)

**Thin far ground cover** (`GraphicsSettings::thin_far_ground_cover`; off at Ultra, on at Low, Medium and High; the editor's Graphics settings) leaves out more of the small painted plants (a box half-diagonal under 3 studs: grass, pebbles, litter) the farther they are: all of them up to 35% of their cull distance, down to a third of them where the cull distance's fade begins (`renderer/lod.rs`, `thin_distance`). Each plant is picked by its ID, so the same ones go and come back, and each fades out by dither over 8% of its cull distance before its own distance, so there's no line where density drops.

Measured with `profile_project`'s `thin-ground-cover` at Ultra 2560 × 1440 against the setting off, on the desktop above (milliseconds; pictures compared pixel by pixel):

| Scene, camera | GPU frame | CPU planning | Pixels changed (more than 8 of 255) |
|---|---|---|---|
| Temperate Dry Patch, demo / ground | −0.05 / −0.03 | −0.40 / −0.58 | none / none |
| General Store Roadside, overview / low | −0.12 / −0.06 | −0.06 / −0.02 | 0.38% / 0.22% |
| Abandoned Farmhouse, front / wide | +0.05 / −0.04 | −0.40 / −0.42 | none / 0.035% |

**It isn't a meaningful saving.** A plant's cull distance comes from its size (120 studs per stud), so a 2.4-stud grass tuft is drawn to 290 studs and only starts thinning at 100: the patches' grass is all nearer than that, and far plants are a few pixels each and cost little on the GPU. Thinning the grass that does cost (the patch's 40–50 layers between 20 and 100 studs) means thinning grass close enough to see.

### Shadows on the patch, cascade by cascade

`RenderStats::cascade_times` (graphics timer stamps before each of the sun's cascades and after the last, inside the Shadows pass; shown in `fps full`, the Profiler's Graphics tab and `profile_project`) with `cascade_casters` and `cascade_triangles`. Demo camera, Ultra 2560 × 1440:

| Cascade (nearest first) | GPU | Casters | Triangles |
|---|---|---|---|
| 1 | 1.22 ms | 5,238 | 2.35 M |
| 2 | 1.53 ms | 15,062 | 3.58 M |
| 3 (every other frame) | 0.87 ms | 15,938 | 3.70 M |
| 4 (every other frame) | 0.61 ms | about the same | |

So a frame's 3.6 ms of shadows is the two near cascades (2.75 ms) plus one far one. Leaving the oaks' shadows out saves 1.9 ms; painted foliage's, 1.0 ms. High's shadows (three 2048² maps, `shadows-high`) cost 2.6–2.9 ms less than Ultra's four 4096² ones. Halving the shadow distance doesn't make them cheaper (+0.4 to +1.4 ms): the same casters land in denser cascades, so the cost follows the texels the casters cover, not the distance.

What was checked and left as it is:

- **The implementation does what it should.** Casters are culled per cascade by their spheres against each cascade's box, the far two cascades take turns past 12,000 casters, painted foliage casts only to 35% of its cull distance, cut-outs cast from their depth-only pipeline (no alpha test, so no fragment shader in the shadow pass), and a cascade is snapped to texels. Nothing is drawn twice or for nothing.
- **Big meshes cut into spatial clusters** (tried, not kept). Cutting every primitive over 65,536 triangles into pieces of at most 32,768 by where its triangles are, so each cascade and the camera cull pieces instead of whole trees, took the first cascade from 2.35 to 1.76 M triangles and the shadow pass from 3.58 to 3.37 ms, but the scene pass got 0.15–0.5 ms slower (2,427 to 2,799 draw calls; the pieces share one distance band, so they lose the sort's near-first order) and the frame didn't get faster (10.81 ms against 10.79–10.88). It would pay for one huge mesh seen partly; the oaks are seen whole.
- **Shadow-specific LOD** was tried before (Small dense patches, above) and rejected for changing how the trees shade themselves. A tree's simplified copy may move its surface up to 2% and 5% of its size, which at the near cascades' texel size (under 0.05 studs) is visible.
- **Caching cascades between frames** isn't worth it here: every plant sways, so its shadow moves every frame, and a moving camera moves the cascades. A scene with no wind and a still camera could keep them; no game benchmark is like that.
- **Lowering Ultra** (fewer or smaller maps) is the one big saving (2.7 ms) and it's a quality choice, which the High preset already offers.

The levers that remain are the scene's: the oaks are 1.7 M triangles, three quarters bark, and the bark material is double-sided. Drawing only the bark one-sided (`profile_project`'s `single-sided-match` with `AXCENITY_PROFILE_MATCH=Bark`) saves 0.22–0.48 ms of GPU across the three cameras, and the pictures match (at most 108 of 3.7 million pixels differ by more than 8 of 255, one by more than 32): the bark is closed, so its back faces are never seen. The owner approved it, so both oaks' Bark materials are one-sided in the project's `.glb` files (originals kept in a backup outside the project). The player's frame on the regression check went from 11.74 to 10.94–11.04 ms (GPU 11.47 to 10.82–10.91 ms, 1% low 70 to 76–78 FPS), and shots from the three cameras match the ones before within the numbers above. Making every double-sided material one-sided would save 0.6–1.5 ms but leaves the backs of leaves missing.

After all of this the full regression check matched every baseline within 7% (Temperate Dry Patch 11.6 ms, GPU 11.3 ms, 1% low 71.6 FPS) and `cargo nextest run --workspace` passed 3,184 of 3,184.

## Levels of detail, inspected

**How levels are picked today** (`renderer/lod.rs`, unchanged by this section): by **projected size**, the share of the view's height an item's bounding sphere covers, not by world distance or screen-space error. Below 30% an item draws its first simpler copy, below 12% its second (`LOD_SCREEN_SIZES`); it switches back once 15% past the threshold (hysteresis), and a switch cross-fades over 12 frames by dither. All primitives of a model share one decision (the sphere of the part's box). Shadow views draw the level the camera picked; only the level fading in casts. Levels come from the file (`_LOD1`, `_LOD2`) or are simplified at load to half and a quarter of the triangles with meshoptimizer, a copy that saves less than a fifth on the level before being dropped ([models.md](models.md)).

**What works:** no popping (dithered fades), no flicker on a threshold (hysteresis, `a_model_sitting_on_a_threshold_keeps_its_level`), switching per model rather than per primitive, levels uploaded only when drawn, and simplified copies cached on disk.

**Limitations, measured on the patches:**

- **Leaf cards don't simplify.** Edge collapse can't merge disconnected quads, so the hazel and juniper shrubs' leaves (840–4,000 triangles) get no copies at all, while their wood does (`lod_report_project`). Grass and other small painted models are under the 256-triangle floor. Their saving comes from the cull distance and `SHADOW_REACH`, not from levels.
- **Thresholds are one size for everything.** A rock and a tree switch at the same screen size; nothing lets an asset category pick its own (`LodReport` and the inspector are written per model, so a per-category table can be added where `lod_level` reads `LOD_SCREEN_SIZES`).
- **Size, not error.** Two copies that look alike switch as early as two that don't; `LodReport`'s Almost the Same warning catches file-made copies like that.
- **Big trees stay at LOD0 in a small scene.** The patches' oaks fill a third of the view or more from anywhere on the patch, so they never switch, and their LOD2 is still 195k–260k triangles.

**The tools:**

- **LOD Levels legend** (View > View Mode > LOD Levels): the viewport's corner names each color (LOD0 green, LOD1 yellow, LOD2 orange; grey has no copies), how many items in view draw each (`RenderStats::lod_items`) and the thresholds. The Overdraw view has a legend too, with a **Measure** button that reads the frame back as `OverdrawStats`.
- **LOD Inspector** (View > Panels > LOD Inspector, `lod_inspector.rs`): for the selected MeshPart, its mesh, camera distance, size on screen, the thresholds, the level it's drawn at (`TextureRenderer::lod_level_of`) and the level its size picks, then each level's triangles, vertices, share of LOD0 and estimated GPU memory, and warnings. **Preview** buttons draw that one part at LOD0, LOD1 or LOD2 in the viewport whatever its size (`TextureRenderer::preview_lod`, no fade), to compare outlines; Auto goes back, and the preview ends when the selection changes or the panel closes. Nothing in the world or the model changes.
- **`assets::LodReport`**: per primitive, its levels' triangles, vertices, share and estimated bytes (position, normal, UV and tangent per corner, 32-bit indices), and warnings, which are advice, never a rejection: no copies for a primitive of 5,000 triangles or more, a level keeping over 85% of the one before, one keeping under 10%, the simplest level still 50,000 triangles or more, and a level whose box shrinks by more than 10% on a side (its outline changes). It's what an import or a generated model can be checked with: the same report applies to any model, whatever its kind.
