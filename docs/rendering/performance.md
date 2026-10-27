# Rendering performance

How drawing is measured, and the baseline that optimizations (culling, LOD, GPU skinning, occlusion) are compared against. Measure before optimizing: a change that claims to make drawing faster shows it here.

## What's measured

- **`RenderStats`** (`TextureRenderer::stats`, `render/src/renderer.rs`), for the last frame: draw calls and triangles (shadow views and the prepass included); items submitted (in the draw list), drawn (by the scene pass) and culled (outside the camera's view); shadow casters drawn into and culled from the shadow views (summed over views) and sprites culled (graphics Phase 12, see Frustum culling in [pipeline.md](pipeline.md)); batches (the scene pass's instanced draws, one per mesh and material); GPU bytes by kind: textures and meshes (counted at upload and when freed), render targets (`FrameTargets`, the shadow maps and the turned-text sheet, counted when made) and buffers; and the GPU's time for the frame and for each pass (`pass_times`, `pass_time(FramePass)`).
- **Per-pass GPU time** (`renderer/gpu_timer.rs`) needs `Features::TIMESTAMP_QUERY`, which the editor and the benchmarks ask for where the adapter has it and the player never does. An empty compute pass writes a timestamp before each `FramePass` (Shadows, Depth prepass, Ambient occlusion, Scene, Post-processing, UI) and one after the last, so a part made of many render passes (three cascades, the bloom chain) is timed whole. A pass the frame didn't draw (shadows or ambient occlusion off, no UI) is `None`. The readings come back a frame or two later without the CPU waiting.
- **The editor's Profiler** (`editor/src/profiler.rs`) shows these while playing: draw calls, batches, triangles, items drawn of submitted, items culled, shadow casters drawn and culled, sprites culled, a **GPU passes** section with each pass's average time ("off" for one no recent frame drew, or one "not available on this GPU" line without timestamps), and a **GPU memory** section by kind.

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
| `bench_2000_mesh_parts_behind_a_wall` | The 2,000 helmets in view but hidden behind an 80-stud wall the camera stands 40 studs from, then the same view with no helmets (added in graphics Phase 15, for occlusion culling) |

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
