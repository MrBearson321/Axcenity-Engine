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
| `bench_20_animated_characters` | 20 copies of hello-animation's Noodle, each with an Animator, posed on the CPU every frame |

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

