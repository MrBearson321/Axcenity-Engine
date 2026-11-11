---
title: Optimization log
type: performance
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [docs/benchmarks.md, docs/rendering/performance.md, benchmarks/baselines.json]
docs: [docs/benchmarks.md, docs/rendering/performance.md, docs/research/engine-modernization-report.md]
tags: [perf, history]
---

# Optimization log

Every significant optimization: the problem, its root cause, the fix, and what it measured. Newest roadmap work first, then the graphics-plan work it built on. Raw tables live in `docs/benchmarks.md` and `docs/rendering/performance.md`; this log keeps the reasoning in one place. Add an entry from `Templates/Optimization-Entry.md`.

**Evidence**: **Measured** = timed, with the method given. **Expected** = reasoned from a measurement. **Theoretical** = from first principles. **Unverified** = a guess to check. Machines: **Desktop** = Ryzen 9 9950X3D, RX 9070 XT, Windows 11, pinned to the V-cache CCD; **Container** = Linux cloud container, 4 vCPU Xeon 2.1 GHz, lavapipe ([Hardware notes](Hardware-Notes.md)). Compare numbers only within one machine.

> [!summary] Net effect (Measured, Desktop, `300bbb5` → `5cb8302`, pinned, alternating)
> S10k frame: test runner 7.71 → 5.03 ms, server 5.13 → 3.82 ms, player 14.93 → 8.73 ms. Temperate Wet Patch test runner 28.97 → 0.048 ms; General Store Roadside player 6.99 → 4.40 ms. The one regression (Abandoned Farmhouse startup) was closed on 2026-11-11 by #65, #67, #69: first frame 4.35 s → 0.21 s. Then physics part 1 and the two thread pools (#84, #86, #88, #93) cut the XB10k cross-engine frame from 6.70 to 5.14 ms.

## Summary table

| Item | PR | Area | Headline (Measured) | Machine |
|---|---|---|---|---|
| DirectX 12 shader compiler and parallel pipelines | #172, #182 | Startup | Wet Patch assets loaded 11.4 → 2.25 s on DirectX 12 (Vulkan 0.98 s) | Desktop |
| Render planning pool | #88 | Render | Unpinned Render step 6–18% less CPU (S10k 2.60 → 2.14 ms) | Desktop |
| Physics own pool | #86, #93 | Physics | Unpinned S10k Physics 2.71 → 2.19 ms, frame 3.37 → 2.81 | Desktop |
| Physics step part 1 | #84 | Physics, events | S10k Physics 6.12 → 4.73 ms, Heartbeat 1.10 → 0.54 (−24% frame) | Container |
| Explorer and Console draw only visible rows | #73 | Editor | See `docs/research/usability/verification.md` | Container |
| Terrain layers on workers, built in parallel | #67, #69 | Startup | Farmhouse first frame 4,354 → 212 ms (with #65); General Store 1,230 → 446 ms | Desktop |
| L2 embedded cook | #65 | Assets | Farmhouse cooked first frame 2,926 → 1,869 ms; GPU textures 947 → 311 MB | Desktop |
| M9 | #61 | Packing | `world.json` ~50× smaller in `.axgame`; cooked games 10–21% smaller | Container |
| L4 query bounds | #59 | Physics queries | S10k Characters 2.05 → 0.31 ms | Container |
| L1 measured | #59 | Core | Name lookups ~9% of S10k instructions: **parked** | Container |
| L3 part 1 | #58 | Scripting | Numeric Luau 3.1× (interpreted), 6.2× (`--!native`) | Container |
| L2 (8 parts) | #45–#57 | Assets | 4096² upload 55–61 → 0.9–1.1 ms; 85 → 21 MiB; cooked model 143 → 1.8 ms | Container |
| M14 | #37 | UI | 500-element inventory layout 27.3 → 3.6 ms per frame | Container |
| M7 | #34 | Network | 8-player network send 45.2 → 5.06 ms; S10k snapshot 14.4 → 1.02 MB | Container |
| M3 | #25 | Physics | S10k Physics 11.32 → 8.87 ms | Container |
| M1 | #15, #16, #20, #24 | Frame | Editor Play S10k 5.99 → 5.09 ms | Desktop |
| M13 | #32 | Navigation | Agents worst 25–36 → 4.0–5.6 ms | Container |
| M6 (4 parts) | #22, #26, #27, #29 | Scripting | Empty Lua call 112 → 25 ns; 10k idle tasks 95,457 → 35 ns/step | Container |
| M4 | #23 | Scene | S10k Draw list 12.9 → 4.3 ms | Container |
| Mesh hulls | #21 | Physics queries | Biome patch Input 30.9 → 0.006 ms | Desktop |
| M5 | #18, #19 | Assets | Arid Dry Patch launch to assets 5,056 → 3,204 ms | Desktop |
| M2 | #17 | Core | S10k test runner frame 6.68 → 5.91 ms | Desktop |
| I11 | #8 | Player startup | hello-roads first frame 2,069–5,431 → 341–398 ms | Container |
| I10 | #9 | Watcher | Reload latency 463 → 103 ms | Container |
| I9 | #11 | Core | S10k Input −15%, Scripts −6 to −10% | Container |
| I8 | #10 | Scene | 2,000 MeshParts draw list 1.84–1.90 → 0.90–0.99 ms | Container |
| I7 | #12 | Navigation | Agents median 4.2–4.6 → 0.36–0.38 ms; worst 120–137 → 26–29 ms | Container |
| I4 | #7 | Scripting | `with_world` 3.3 → 0.15 µs | Container |
| Threaded planning | 3f2fc05 | Render | Forest 1440p Ultra CPU draw 39.6 → ~9.7 ms, GPU 31 → 13.7 ms | Desktop |
| GPU skinning | graphics Ph. 14 | Render | 20 walking characters CPU draw 4.85 → 0.38 ms | Desktop |
| Culling, LOD | graphics Ph. 12–13 | Render | Edge views draw only what's seen; foliage cull distances | Desktop |

## Entries

### DirectX 12 startup (2026-11-11)

| | |
|---|---|
| **Roadmap item / PR** | M12 follow-ups, PR #172 (static DXC) and #182 (parallel scene pipelines) |
| **Evidence** | Measured |
| **Problem** | After #104 made DirectX 12 the default on Windows, projects took about 8.5 s longer to load than on Vulkan |
| **Root cause** | Without DXC, wgpu compiled every shader with the old FXC. With DXC, building pipelines still costs about 0.9 s at every launch, because DirectX 12 has no cache between launches (Vulkan's driver keeps one: 76 ms) |
| **Solution** | Link DXC in (wgpu `static-dxc`, needs Visual Studio's ATL); make the scene's ten pipelines on threads of their own (`gpu::in_parallel`) |
| **Affected components** | `Cargo.toml`, `crates/render/src/renderer/scene.rs`, `gpu.rs` |
| **Before → after** | Temperate Wet Patch launch to assets loaded 11.4 s (FXC) → 2.41 s (DXC) → 2.25 s (parallel); pipelines ready 1,123 → 886 ms. Farmhouse 10.1 → 2.60 → 2.36 s. Vulkan 0.98 s |
| **Method** | Release player, `--bench 300`, one warm-up then medians of 3 |
| **Hardware** | Desktop |
| **Trade-offs** | About 21.5 MB more per Windows executable; Windows builds need the ATL component. About 0.4 s more than Vulkan is still unexplained (maybe Vulkan and GL adapter listing) |
| **Detail** | `docs/rendering/pipeline.md`, "Graphics API" |

### Physics step and thread pools (2026-11-11)

| | |
|---|---|
| **PRs** | #84 (part 1), #86 and #93 (own pool), #88 (render pool), #99 (contact recycle, behavior) |
| **Problem** | The engine comparison (XB10k, #80) found physics the largest cost of a frame; about 40% of `Physics::step` was Axcenity's own work around rapier |
| **Root cause** | Resetting forces on every body; touches in `BTreeSet`/`BTreeMap` with a part lookup per contact; property writes looked up by name and allocating; `Changed` lookups for ~4,400 writes per frame on both sides; rayon's global pool of 32 threads oversubscribing rapier and draw planning |
| **Solution** | Reset only pushed bodies; sorted merge with a per-collider owner table; `set_known_property`; motion-only re-reads; skip unwatched Instances in `changes::fire`; own pools of at most 12 threads for physics and rendering |
| **Before → after** | Container S10k test runner: Physics 6.12 → 4.73 ms, Heartbeat 1.10 → 0.54, frame 8.07 → 6.09. Desktop unpinned: Physics 2.71 → 2.19, frame 3.37 → 2.81; S10k Render step 2.60 → 2.14 |
| **Rejected** | Fewer solver iterations or no stabilization: up to −20% but stacks fell and chains stretched |
| **Side fix** | Torque movers' torques used to pile up every step |

### Terrain layers off the main thread (2026-11-11)

| | |
|---|---|
| **PRs** | #67, #69 (with #65), recorded in #70 |
| **Problem** | After #65, ~1.2 s of Farmhouse's first frame was the terrain decoding 30 cooked 2K layer maps on the main thread (`Image::rgba()`) |
| **Solution** | Ask for layer maps with their pixels on the loading workers; build layer arrays in parallel, resizing each texture once |
| **Before → after** | Desktop baselines `39dd258` → `15965da`: Farmhouse first frame 4,354 → 212 ms, assets in 6,043 → 1,579 ms; General Store first frame 1,230 → 446; Wet Patch 820 → 268 |

### L2 embedded cook

| | |
|---|---|
| **PR** | #65 (`924720a`, merged in `21aadbf`) |
| **Problem** | Abandoned Farmhouse's 29 `.glb` files embed all their 4K maps; the cook skipped images inside models, so they decoded and mipmapped on the first frame |
| **Root cause** | Cooking only covered texture *files*; embedded images needed an import to copy them out |
| **Solution** | Cook embedded images in place, one shared copy per distinct image per project (`embedded-<hash>.axtex`), shared `Image` in memory |
| **Before → after** | main uncooked → branch cooked: first frame 2,926 → 1,869 ms; launch to assets 7,224 → 3,330 ms; steady frame 5.449 → 5.413 ms; GPU textures 947 → 311 MB |
| **Method** | Player `--bench`, release, pinned, medians of 3; parity shots max diff 9/255 |
| **Trade-offs** | Per-model copies (first version) used 1,764 MB on disk and more GPU memory; fixed by sharing |
| **Left** | ~1.2 s of first-frame Render is the terrain decoding 30 cooked 2K layer maps on the main thread (`Image::rgba()`) |

### M9: compressed game files

| | |
|---|---|
| **PR** | #61 |
| **Problem** | `.axgame` v1 stored files raw; unpacking reset modified times so cooked copies were re-hashed |
| **Solution** | v2: zstd level 3 per entry when it saves ≥ 5%, restored mtimes, unpacking on up to 8 threads |
| **Before → after** | hello-roads 830 → 745 MB, unpack 210 → 166 ms; S10k 10 → 0.2 MB |
| **Trade-offs** | Packing slower (hello-roads 1.0 → 3.3 s). Read-in-place not done: unpacking is only 30–170 ms |

### L4 query bounds

| | |
|---|---|
| **PR** | #59 |
| **Problem** | L4 (parallel stages) was to be guided by data; S10k's Characters step was 2.05 ms with one character |
| **Root cause** | Callgrind: 93% in `Reach::may_touch`: every ray walked all 10,000 parts reading `Position` and `Size` by name |
| **Solution** | Keep each part's bounding ball between queries (`physics::query::WorkspaceBounds`), checked by revision. Fixing the scan beat threading it |
| **Before → after** | Test runner Characters 2.05 → 0.31 ms; frame 10.03 → 8.56 ms |
| **Method** | Container, alternating, three 600-frame runs |

### L1 measured, parked

| | |
|---|---|
| **PR** | #59 (second commit; recorded only in the PR body, see [Documentation history](../Changelog/Documentation-History.md#open-documentation-gaps)) |
| **Question** | Do string-keyed properties still dominate after M2 and M4? |
| **Result** | Callgrind over whole S10k frames: physics 58%, navmesh refresh 19%, `fire_changes` 11%, name lookups ~9% of instructions |
| **Decision** | L1 (typed per-class slots, ~500 call sites) not worth it now |

### L3 part 1: fast builtins and opt-in native code

| | |
|---|---|
| **PR** | #58 |
| **Root cause** | Without `safeenv`, Luau looks up every `math.sin` and calls through the C API; native code bails out at each such call (`--!native` alone measured 0.99×) |
| **Solution** | Read-only per-script libraries + `safeenv`; `--!native` header compiles with `luau-jit` |
| **Before → after** | 3M `math.sin(i) * math.sqrt(i)`: 384–387 → 124–127 ms (interpreted) → 62–63 ms (native) |
| **Not changed** | Engine calls: Vector3 math and Instance reads stay mlua userdata calls; S10k frame unchanged |

### L2: cooked textures

| | |
|---|---|
| **PRs** | #45 (BC7), #47 (ship in builds), #49 (LOD cache), #50 (asset IDs), #51 (cooked models), #53 (orphans), #55 (cook on change), #57 (BC5 normals) |
| **Problem** | First frames spent seconds decoding JPEGs, making CPU mips and uploading RGBA8; models parsed FBX/glTF and simplified LODs on every load |
| **Solution** | Editor cooks offline: BC7 mips (`intel_tex_2`), BC5 normal maps, `.axlod` LOD cache, whole `.axmodel` models |
| **Before → after** | 4096² map: upload 55–61 → 0.9–1.1 ms, worker read ~650 → ~30 ms, GPU 85 → 21 MiB. Simplifying CityProps 68 → 6 ms. Cooked model CityProps 143 → 1.7–1.9 ms, Walking.fbx 178 → 19–24 ms |
| **Trade-offs** | Built games carry sources and cooked copies (hello-roads 393 → 830 MB). BC7 "very fast" PSNR 35.8 dB color; normal maps needed BC5. lavapipe shows no first-frame gain (software BC7 decode) |

### M14: UI layout cache

| | |
|---|---|
| **PR** | #37 |
| **Root cause** | Callgrind: the CSS cascade was 88% of a frame; each element matched every rule against itself and its ancestors twice a pass; editor Play laid UI out four times |
| **Solution** | `StyleCache` per frame, inherited custom properties shared, selectors matched once per rule, a reused `TaffyTree` per thread, `hit_layout` reused as the first pass |
| **Before → after** | One pass 12.8–13.5 → 1.88–1.90 ms; player frame 27.3–28.2 → 3.58–3.66 ms; editor Play 53.4–55.8 → 3.60–3.69 ms |

### M7: replication wire

| | |
|---|---|
| **PR** | #34 (protocol 10) |
| **Problem** | Nobody could join S10k: its 14.4 MB JSON snapshot overflowed the 5 MiB reliable channel; network send with 8 players was 45 ms |
| **Root cause** | 92% of send in `MotionSender::frame`, re-reading 10,000 parts by name per player per frame; the diff cloned every moving part's properties |
| **Solution** | postcard wire, creates carry only non-default properties, motion re-read only when `motion_revision` moved, diff skips motion-only changes |
| **Before → after** | Snapshot 14.4 → 1.02 MB, join 242 → 19 frames, send 45.2 → 5.06 ms, frame 57.4 → 14.9 ms, bytes/frame 10,324 → 5,464 |

### M3: physics sync

| | |
|---|---|
| **PR** | #25 |
| **Root cause** | Physics re-read what it wrote itself; any reparent forced a full resync; any script change woke every body; write-back each substep converted quaternions to Euler |
| **Solution** | Physics-owned revisions, per-service structure versions, wake neighbours, write back once per frame (awake bodies), rapier `parallel` |
| **Before → after** | Physics 11.32 → 8.87 ms (test runner), 9.91 → 8.15 ms (server); sync 1.6–1.7 → 0.7 ms; rapier's step −17% |
| **Trade-offs** | `parallel` costs tiny scenes (`hello-vehicle` 0.05–0.07 → 0.19 ms) |

### M1: one frame schedule

| | |
|---|---|
| **PRs** | #15, #16, #20, #24 |
| **Problem** | Four hand-ordered loops had drifted (correctness, not speed) |
| **Measured side effect** | Editor Play S10k frame 5.99 → 5.09 ms, Physics 3.34 → 2.29 ms, worst 13.8–17.2 → 5.9–6.9 ms (seven pinned alternating rounds); an earlier container hint of a 5.9% slowdown was noise |

### M13: navmesh on a worker

| | |
|---|---|
| **PR** | #32 |
| **Solution** | `MeshBuilder::look` reads changes on the game thread; `navmesh build` thread assembles; one `SharedNavMesh` for agents and Humanoids |
| **Before → after** | Agents worst 25–36 → 4.0–5.6 ms, mean 0.89–1.07 → 0.33–0.38 ms; median unchanged |

### M6: script hot paths

| | |
|---|---|
| **PRs** | #22 (heap scheduler, prune on destroy), #26 (watchdog `Cell`, class method tables), #27 (chunk cache), #29 (handler coroutine reuse) |
| **Before → after** | 10k idle tasks 95,457 → 35 ns/step; step with 10k handlers 59,363 → 2,949 ns; empty Lua call 112 → 25 ns; `part.Position` read 512 → 397 ns; 200 script copies 2.4 → 0.2 ms each; handler call −18% instructions |
| **Rejected** | `mlua::LuaString` keys: slower with mlua 0.12 (5,869 vs 5,417 instructions) |

### M4: retained draw items

| | |
|---|---|
| **PR** | #23 |
| **Root cause** | Every frame re-derived every part's `DrawItem` from properties |
| **Solution** | `DrawCache` keyed by revision plus recorded dependencies; randomized equality tests against from-scratch |
| **Before → after** | `bench_build_10000_parts` 6.4–8.0 → 1.2–1.5 ms; S10k Draw list 12.9 → 4.3–4.4 ms |
| **Left** | Copying kept items (~1.2 ms per 10k); next: spatial `ItemChunk`s |

### Mesh hulls and the mouse ray

| | |
|---|---|
| **PR** | #21 |
| **Root cause** | `Clicks::step` cast the mouse ray every frame; each query built a new convex hull of every corner of nearby MeshParts (2–3 M corners in the biome patches' scanned trees) |
| **Solution** | Hulls kept by model and size (`shapes::hull`); no ray without a ClickDetector |
| **Before → after** | Arid Dry Patch Input 30.9 → 0.006 ms, frame 31.4 → 0.21 ms (test runner); player frame 44.9 → 6.7 ms |

### M5: loading off the lock

| | |
|---|---|
| **PRs** | #18, #19 |
| **Root cause** | Parsing, decoding and simplifying held `Assets`' global mutex; models loaded one at a time |
| **Solution** | Lock only to look up and store; worker threads; `LoadTicket`; draw nothing until a model is in |
| **Before → after** | Arid Dry Patch launch to assets 5,056 → 3,204 ms, worst frame 168 → 59 ms |
| **Trade-off** | Texture work landed on the first frame instead (Farmhouse regression, later addressed by L2 and #65) |

### M2: class and tag index

| | |
|---|---|
| **PR** | #17 |
| **Root cause** | ~15–20 full Workspace walks per frame across systems, each allocating |
| **Before → after** | Desktop S10k frame: test runner 6.68 → 5.91 ms, server 4.88 → 4.65, player 15.48 → 15.26 |
| **Note** | Small worlds gain nothing measurable (six real projects within noise) |

### I11, I10, I9, I8, I7, I4

| Item | Root cause → fix | Measured |
|---|---|---|
| I11 (#8) | First frame decoded every texture on the main thread → preload thread from project load | hello-roads first frame 2,069–5,431 → 341–398 ms |
| I10 (#9) | 0.5 s polling walk → `notify` events, polling fallback | Reload latency 463 → 103 ms (100 ms is the settle wait) |
| I9 (#11) | Allocating `descendants()`, SipHash on ID keys → `descendants_iter`, `FxHashMap` (needed `#[inline]`, else Scripts got 40% slower) | Input −15%, Scripts −6 to −10.5%, Characters −6.5% |
| I8 (#10) | A new mesh and LOD copies per MeshPart per frame → shared per model (`mesh_cache.rs`) | 2,000 helmets draw list 1.84–1.90 → 0.90–0.99 ms |
| I7 (#12) | Navmesh fingerprint hashed moving drops via `format!` → anchored ground only, incremental builder | Agents median 4.2–4.6 → 0.36–0.38 ms, worst 120–137 → 26–29 ms |
| I4 (#7) | `with_world` built a 14-Instance default World per call → swap into a `LentWorld` slot | 3.3 → 0.15 µs per call; S10k Scripts 0.391 → 0.361 ms |

### Threaded draw planning (3f2fc05)

| | |
|---|---|
| **Problem** | Painted forest at 1440p Ultra: 26 fps, 40.8 ms CPU on 1 of 32 threads |
| **Root cause** | `LodStates::resolve` 5.2, material keys 3.4, `upload_new` 8.4, `plan_draws` 12.9 ms; hashing a 330-byte `MaterialKey` twice per draw; every leaf lit then covered |
| **Solution** | rayon above 4,096 items in fixed-order shares; per-thing not per-draw work; cut-out depth fill; far cascades alternate past 12,000 casters; cheaper leaf sun visibility |
| **Before → after** | CPU `ScenePipeline::draw` 39.6 → 9.5–10.0 ms; GPU 31.0 → 13.6–13.9 ms; picture identical within noise |

### Graphics plan (Phases 12–15)

- **Culling (12)**: before it, edge views drew everything and shadows were 43–52% of the GPU frame. **LOD (13)**: 154.5 M triangles a frame for 2,000 helmets called for it; foliage cull distances by kind. **GPU skinning (14)**: 20 walking characters 4.85 → 0.38 ms CPU, mesh memory 13 → 2.3 MB. **Texture budget (15)**: caps texture memory by preset. Tables: `docs/rendering/performance.md`.
