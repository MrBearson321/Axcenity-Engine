# Engine Modernization Report

An architectural audit of Axcenity Engine benchmarked against Unreal Engine 5, Godot 4, Unity 6 and Roblox Studio, with package recommendations, patterns to port and a three-tier refactoring roadmap.

- **Date:** 2026-11-10
- **Codebase:** `main` at `556b72b` ("Leaf Crowns and Gentler Real-Time GI in Heavy Scenes"), 25 crates, about 470k lines of Rust plus 5.1k lines of WGSL.
- **Method:** three code auditors read the source in parallel (runtime core, simulation, tooling and loops), one researcher gathered facts on the four peer engines from official documentation, then a benchmarking pass and a strategy pass built on those notes. Key claims were spot-checked against the repo a second time.
- **Not done:** nothing was built, run or timed. Every performance figure is either quoted from `docs/rendering/performance.md` and `docs/architecture/streaming.md`, or is an inference. Roadmap item **I1 (measure first)** exists because of this.

> **Update, 2026-11-10: open questions answered.** Paul answered the questions in section 7 and set a [development policy](../development-policy.md). In short: GitHub Actions is intentionally off and testing is local; cross-machine determinism is not a requirement; desktop (Windows and Linux) comes first, with mobile and web deferred; the scripting API is Roblox-inspired, with Vector3 a first-class type and native `vector` not chosen until its type identity and interop are verified; internet-hosted dedicated servers are a future objective. The roadmap in section 6 is marked to match. Where the audit text below mentions CI as a running check, read it as a description of the workflow files, not of anything that runs today.

**Evidence markers used throughout**

| Marker | Meaning |
|---|---|
| **[V]** | Read in the Axcenity source, with `file:line` (paths are under `crates/` unless they start with `docs/`, `.github/` or `Cargo.toml`) |
| **[I]** | Inference from the code, not measured |
| **[F]** | Peer-engine claim taken from an official page that was read |
| **[S]** | Official page found by search but not read in full |
| **[K]** | Peer-engine claim from general knowledge, not fetched. Check before quoting |
| **[cr]** | Crate version, licence or feature read from the crates.io API on 2026-11-10 |
| **[verify]** | Open question to confirm before acting |

---

## 1. Executive summary

Axcenity has the right *shape* for a Roblox-style engine. It has a Roblox-familiar Instance tree in a generational arena, deferred change events with Roblox's 10-round depth cap, a renderer that only sees plain draw data (Godot's "servers" idea), documented frame ordering, text project files that diff cleanly, strong crash isolation, and a CI that enforces the crate layering. The renderer is the most mature subsystem: clustered lights, instanced batching, frustum culling per shadow view, LOD hysteresis, GPU skinning and particles.

What holds it back is that **almost every runtime system pulls the whole world every frame instead of reacting to change**, and there is **no asset pipeline**. Concretely:

1. **No class or tag index.** About 15–20 full Workspace walks per frame across scene, physics, audio, anim, nav, procedural and script, and each `descendants()` call allocates a `Vec` (`core/src/world.rs:1194`) [V; count I].
2. **The draw list is rebuilt from scratch every frame** (`scene/src/draw_list.rs:449-529`) [V], even though per-Instance `revision` and `structure_version` already exist and physics and net use them.
3. **Properties are a `BTreeMap<&'static str, Value>` per Instance** (`core/src/world.rs:55`) [V], so every read is string comparisons, and each Lua property access allocates a `String` and a new userdata (`script/src/instance.rs:114-121`) [V].
4. **Asset loads are synchronous and run under one global mutex** (`assets/src/lib.rs:107-109, 236-264`) [V]. The player never turns on the background texture workers, so first-sight decodes land on the main thread.
5. **Four hand-ordered game loops** (player, editor Play, server, test runner) have **already drifted apart** [V]. Something that works in editor Play can behave differently in a shipped game.
6. **Two god-crates.** `editor` is 162k lines in one crate, and `script` depends on 11 engine crates, which makes the headless server link wgpu and cpal [V manifests; I link].

There is also **one likely correctness bug**: Instances can't be used as Lua table keys (`t[part]`, `data[player]`), because every access creates a fresh userdata and Lua table lookup uses raw identity, not `__eq` (`script/src/types.rs:954-958`, `script/src/instance.rs:122`) [V code shape; no test exists yet].

**Scorecard (1 = weak, 5 = industry-leading)**

| Pillar | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| Scene hierarchy & ECS | **2.5** | 5 | 4 | 4.5 | 4 |
| Assets & streaming | **2** | 5 | 3.5 | 4.5 | 4 |
| Runtime performance & execution | **2.5** | 5 | 3.5 | 4.5 | 4 |
| Usability & extensibility | **3** | 4 | 4.5 | 4.5 | 4 |

**Top recommendations, in order**

1. **Measure first** (I1): a headless 10k-part simulation benchmark plus `profiling` spans in all four hosts. The `profiling` crate is already in the lockfile through wgpu, egui and rapier.
2. **Fix the Lua table-key bug** (I2) with a weak-valued userdata cache per VM. One to two days.
3. **Add a class and tag index to `World`** (M2). Removes most of the per-frame walks and unblocks everything below.
4. **One shared, data-driven frame `Schedule`** using Roblox's RunService stage names (M1). Ends host drift and gives every stage a profiler span.
5. **Retained draw items keyed by `revision`** (M4): Godot's servers pattern applied to the existing `DrawList` boundary.
6. **Asset loading off the lock, then a Godot-style import pipeline** with stable IDs and BC7 textures (M5 then L2).
7. **Do not adopt an ECS** (bevy_ecs, Mass-style) for the Instance tree. Unity's own documentation shows archetype fragmentation on exactly the kind of heterogeneous, prefab-heavy content Roblox-style games produce [F].

---

## 2. Phase 1: Codebase audit

### 2.1 Subsystem map

| Subsystem | Crates (LOC) | Core types | How it works today |
|---|---|---|---|
| **Memory / Core** | `core` (11.2k) | `World`, `Instance`, `Arena<T>`, `Id{index,generation}`, `Value` (16 variants), `ClassName` | One owned `World`, passed by `&`/`&mut`, no locks. Generational arena with a second ID space above 2³¹ for replica-local Instances (`core/src/arena.rs`, `world.rs:25-47`). Properties are a string-keyed `BTreeMap` per Instance; about 28 entries and an estimated ~2 KB of heap per Part [I]. Per-Instance `revision` and a global `structure_version`. Opt-in `ChangeLog` (`world.rs:206-253`). |
| **Scene graph** | `core`, `scene` (11.3k) | `build_draw_list*`, `TerrainCache`, `PaintedFoliage`, `Streaming` | Roblox-style Instance tree, no ECS. `scene` walks the Workspace each frame and emits `DrawItem`s, lights, probes and decals. Only terrain chunks and painted foliage are cached (`ItemChunk`). |
| **Rendering** | `render` (34.7k + 5.1k WGSL) | `DrawList`, `DrawItem`, `ItemChunk`, `MeshSource`/`TextureSource`, `Renderer`, `TextureRenderer`, `ScenePipeline` | wgpu 25. Instance-free contract: meshes and textures arrive as `Arc<dyn …>` keyed by `u64`. `ScenePipeline::draw` sequences every pass by hand (`renderer/scene.rs:532`). rayon-parallel planning above 4,096 items. Instanced batches by mesh and material, per-view frustum culling, LOD with hysteresis, GPU skinning and particles, texture budget. No indirect draws, no render graph, RGBA8 textures only. |
| **Asset pipeline** | `assets` (11.9k), `project` (15.1k) | `Assets` (`Arc<Mutex<Shared>>`), `Model`, `Primitive`, `ImportMeta` (none yet) | glTF via `gltf`, FBX via `ufbx`, textures via `image`, LODs via `meshopt`, all parsed at runtime, lazily, on first use, under the global lock. Paths are the asset identity; there are no UIDs. Live reload by (mtime, size) keeps the last good version. `.axgame` packs ship raw source files, uncompressed, and are unpacked to a temp folder on every launch. |
| **Physics** | `physics` (8.4k), `character` (4.1k), `nav` (4.2k) | `Physics`, `Body`, `PartState`, `Controllers`, `NavMesh`, `Agents` | rapier3d 0.36 with no extra features. Fixed 60 Hz accumulator, at most 10 steps (`physics/src/lib.rs:93, 535`). Welded assemblies become one body. World↔rapier sync is dirty-checked per body revision, but any reparent forces a full resync, and any script change wakes every body (`lib.rs:975`). Kinematic characters use ray probes. Navmesh rebuilds synchronously when its fingerprint changes. |
| **Scripting** | `script` (49.5k, ~26k runtime), `blocks` (8.1k), `blocks-runtime` (6.8k), `plugins` (1.5k) | `ScriptRuntime` (~40 fields), `Context`, `LuaInstance(InstanceId)`, `Connections`, scheduler | Luau through mlua 0.12, two VMs per runtime (server and client). Strong allowlist sandbox per script and a 3 s watchdog. `with_world` moves the whole World into Lua app data for each call (`lib.rs:2371`). Scheduler is a `Vec` partitioned every step; each handler fire creates a new coroutine. Blocks and node graphs translate to Lua at load. `script` also hosts most gameplay systems (health, tools, humanoids, quests, dialogue, saves, sequences). |
| **Audio** | `audio` (2.9k) | `GameAudio`, `AudioEngine` | kira 0.12. Lazily opened output; applies queued `SoundAction`s and syncs every Sound each frame; buses, effects, ReverbZones; camera is the listener. |
| **Animation** | `anim` (11.7k) | `Animators`, `Clip`, `Machine`, `Rig`, `Pose` | State machines with layers and 2D blend spaces, IK, foot placement, ragdolls, cutscene sequences. CPU-only. |
| **Networking** | `net` (8.2k), `server` (0.4k) | `Network`, `Replicator`, `MotionSender`, `Smoother` | renet 2 over UDP or in-memory loopback. Replication diffs the world against what was sent; reliable updates are JSON. Motion is quantized, budgeted and prioritized, with server-side movement checks. netcode runs in Unsecure mode. |
| **UI** | `ui` (15.2k) | `UiInteraction`, layout | taffy layout plus a CSS cascade; glyphon text. Rebuilds the cascade and a new taffy tree per container every frame (`ui/src/lib.rs:1164`). |
| **Editor & tools** | `editor` (162k), `testing` (1.7k), `mesh_edit` (22.8k), `procedural`, `save`, `terrain` | `EditorApp` (109 fields) | eframe/egui 0.32 with egui_dock. Viewport renders on eframe's own wgpu device. Play runs a cloned world with `catch_unwind` per frame and multi-client Play over loopback. Polling file watcher, engine rebuild-and-relaunch, and subsecond hot-patching behind a dev feature. |

### 2.2 How the main loop runs

The order is written down only as prose in `docs/architecture/frame-loop.md`. Each host copies it by hand:

| Host | Where the frame is ordered |
|---|---|
| Player | `player/src/main.rs:1424-1825` |
| Editor Play | `editor/src/play.rs:507-760`, plus particles and streaming inside viewport drawing (`editor/src/viewport.rs:1257-1362`) |
| Dedicated server | `server/src/main.rs:175-208` |
| Test runner | `testing/src/game.rs:129-230` |

The player's frame, simplified:

```
0  network.receive                              main.rs:1455
1  characters.step, agents.step                 main.rs:1496-1515
2  scripts.step, stepped                        main.rs:1543-1545
3  keep_kinematic → physics.step                main.rs:1566-1570
4  fire_touched, heartbeat, audio, animators    main.rs:1578-1619
5  streaming, build_draw_list (+UI layout)      main.rs:1666-1669
6  network.send                                 main.rs:1796
7  renderer.render                              main.rs:1808
```

**Drift that already exists [V]:** editor Play runs the render step before the Animators update, while the player and test runner do the opposite. Audio sits in a different place. In the editor, particles and world streaming advance inside viewport drawing rather than in the game frame. The overlay UI (hotbar, abilities, quests, dialogue, leaderboard, chat, prompts) is assembled by near-identical code in `player/src/main.rs:1698-1789` and `editor/src/viewport.rs:1299-1355`.

### 2.3 Dependency and provider matrix

All versions are pinned once in the root `Cargo.toml`; lock versions in brackets.

| Feature | Provider (version) | Used by | Hook into the frame |
|---|---|---|---|
| GPU | `wgpu` 25 [25.0.2] | `render`, `editor` (through eframe) | `Renderer::render` → `ScenePipeline::draw` (`render/src/renderer.rs:334`, `renderer/scene.rs:532`); editor shares eframe's device (`renderer.rs:370`) |
| Window and input | `winit` 0.30 [0.30.13], `gilrs` 0.11 | `player`, `editor`, `ui` | Event loop in the player; controllers for UI focus |
| Text | `glyphon` 0.9 | `render` | `prepare_ui_text` inside `draw` (`renderer/scene.rs:918`) |
| Math | `glam` 0.34 (5 versions in the lock: 0.30–0.34) | everywhere | — |
| Parallelism | `rayon` 1.10 [1.12.0] | `render` only | `renderer/parallel.rs:12-38` |
| Hashing | `rustc-hash` 2 | `render` per-frame maps | Long-lived caches still use SipHash |
| Models | `gltf` 1.4, `ufbx` 0.11, `meshopt` 0.7 | `assets` | Lazily on first `Assets::model()`, usually inside `build_draw_list` (`scene/src/draw_list.rs:1070`), under the mutex |
| Textures | `image` 0.25, `png` 0.18 | `assets`, `terrain` | Main thread unless `load_textures_in_background` is called (editor only, `editor/src/app.rs:1183`) |
| Physics | `rapier3d` 0.36 [0.36.1], no features | `physics` | `physics.step` (`player/src/main.rs:1570`) |
| Scripting | `mlua` 0.12 [0.12.2] `luau`; `luau0-src =0.22.0` | `script` | `scripts.step`, `heartbeat`, `fire_*` (`main.rs:1543-1589`) |
| Audio | `kira` 0.12 [0.12.5] (brings `cpal`), `mint` | `audio` | `audio.update` (`main.rs:440`) |
| Networking | `renet`, `renet_netcode` 2 [2.0.0] | `net` | `receive` first, `send` before render |
| UI layout | `taffy` 0.7 | `ui` | `UiInteraction::layout` inside draw-list build |
| Editor shell | `eframe` 0.32, `egui_dock` 0.17, `egui-phosphor` 0.10, `rfd` 0.17 | `editor` | eframe owns the editor loop |
| Downloads | `ureq` 3 (rustls), `zip` 2 | `editor` texture catalog | On demand |
| Hot-patch (dev) | `subsecond`, `dioxus-devtools`, `dioxus-cli-config` 0.7 | `editor` behind `hot-patch` | Dev only |
| Serialization | `serde`, `serde_json` | everywhere | Project files, net wire, saves |
| Build | `cargo-hakari` (`workspace-hack`) | all crates | Build-time feature unification |

**Fragile pins and couplings [V]:**
- `luau0-src = "=0.22.0"` (`Cargo.toml:77-84`). mlua-sys 0.13.0 reads Luau bytecode only up to version 11, but luau0-src 0.22.1 writes version 13. mlua-sys declares `^0.22.0`, so the upstream range is wrong [cr]. The exact pin is the correct defence until a new mlua-sys ships.
- eframe 0.32, egui_dock 0.17, egui-phosphor 0.10, wgpu 25 and winit 0.30 can only move together, because the editor viewport renders on eframe's wgpu device.

### 2.4 Bottlenecks and friction

**Ranked hotspots (CPU per frame, scaling with instance count)**

| # | Hotspot | Evidence | Why it matters |
|---|---|---|---|
| 1 | Full-tree walks for "all X" | `core/src/world.rs:1194` (allocating `descendants`), `world.rs:1142-1160` (`tagged()` O(N)); call sites in physics `lib.rs:961-968`, audio `lib.rs:406`, anim `animators.rs:290`, character `lib.rs:746`, nav `agents.rs:408`, procedural `lib.rs:68`, about 30 in `script` [V] | Every system pays O(Instances) per frame, plus a `Vec` allocation per walk |
| 2 | Full draw-list rebuild | `scene/src/draw_list.rs:449-529` [V]; 3.3 ms for 10k parts, 6.1 ms for 50k streamed (docs) | The scene-side cost grows with the world, not with what changed |
| 3 | MeshPart per-frame allocation | Global asset lock taken twice, new `Arc<ModelMesh>`, LOD cell `Vec` and `Arc<Skin>` per MeshPart per frame (`draw_list.rs:1070-1130, 1372`) [V] | Throws away the LOD cache every frame |
| 4 | String-keyed property reads | `world.rs:55`; Lua `__index` copies the key to `String`, runs a ~9-step string chain, then linearly scans class methods (`script/src/instance.rs:114-249, 466-474`) [V] | Paid per read, per system, per frame |
| 5 | Physics sync coarseness | Physics' own writes bump revisions, so moving parts are re-read every frame; global `structure_version` forces full resyncs; `wake_up_all` on any script change (`physics/src/lib.rs:782-905, 975`); `write_to_world` inside the substep loop converts quaternions to Euler each step (`lib.rs:561`) [V] | Sleeping is defeated; resync cliffs on any reparent |
| 6 | Change log flooding | Each physics write becomes a recorded change passed to both Lua VMs; firing DescendantAdded is O(n²) when a big model is cloned in (`script/src/changes.rs:91-104`) [V] | Scripts pay for physics motion they never listen to |
| 7 | Scheduler and events | `take_due` partitions the whole wait list every step (`scheduler.rs:101`); a new Luau thread per handler fire (`events.rs:240`); all connections pruned each frame (`context.rs:385`) [V] | O(waiting tasks) per frame, allocation per event |
| 8 | `with_world` | Each Lua call does `std::mem::take(world)`, which builds a full default DataModel with 13 services (`script/src/lib.rs:2371`, `core/src/world.rs:300`), at about 28 call sites [V] | Pure overhead on every call into Lua |
| 9 | Replication diff | Every moving part gets a full clone of properties, attributes and tags plus an O(P²) diff each frame (`net/src/replicate.rs:233-249, 517-528`); reliable updates and snapshots are JSON [V] | Server cost grows with moving parts × players |
| 10 | Navmesh hitch | The fingerprint hashes every part (including unanchored moving ones), so a moving crate triggers a synchronous ~50 ms `NavMesh::build` every 0.5 s (`nav/src/agents.rs:36, 335`) [V; 50 ms from Cargo.toml comment] | Periodic frame spikes whenever agents exist |
| 11 | UI layout every frame | New taffy tree and full CSS cascade per container per frame (`ui/src/lib.rs:1164`); extra full layout on each mouse press (`player/src/main.rs:901`); game UI laid out twice per editor frame (`editor/src/viewport.rs:1052`) [V] | Large inventories and HUDs scale badly |
| 12 | Single-threaded simulation | rapier has no `parallel` feature; rayon is only in `render`; the only other threads are CSG, texture loading, upload and terrain chunks [V] | Multi-core CPUs sit mostly idle |

**Asset-side friction [V]**
- Model parse, embedded texture decode and meshopt simplification all run while holding `Arc<Mutex<Shared>>` (`assets/src/lib.rs:232-264`), blocking every other asset caller, including the texture workers.
- Mipmaps are built on the CPU with a scalar box filter (`render/src/renderer/gpu.rs:967`); textures go up as uncompressed RGBA8 and the decoded pixels stay in CPU memory too.
- Background GPU uploads only happen when `StreamingEnabled` is on (`player/src/main.rs:839-843`).

**Architectural friction [V]**
- `script` is a god-crate: 11 engine dependencies, ~20 `step`/`fire` calls hand-ordered in four hosts, and `net` reaches into `ScriptRuntime` through about 30 methods. It uses `render` only for three plain types and `audio` only for `SoundAction` and some enum lists, yet those edges make the headless server link wgpu, cpal (ALSA on Linux), image and meshopt.
- `editor` is one crate of 162k lines; `EditorApp` has 109 fields across about 80 `impl` files; incremental rebuild about 5–6 s, cold build about 6 min (`docs/development.md`).
- egui repaints the whole editor every Play frame (`editor/src/app.rs:1305`); the Explorer lays out every row every frame (`explorer.rs:286`); the frame pacer spin-waits up to 2 ms on the UI thread (`app.rs:1079`); file dialogs block Play.
- The plugin API is Lua-only and small: no property editors, importers, exporters, gizmos or asset types, and no native extension point.

**Build, CI and release [V]**
- `rust-toolchain.toml` says `stable` while CI builds with `-D warnings`, so a new compiler lint can break CI with no code change.
- All benchmarks are `#[ignore]` and none covers scripts, moving physics, the change log, replication or nav.
- GPU tests print "skipped: no graphics adapter" and pass, so they probably don't run on Linux CI [I].
- No `[profile.release]` tuning (no LTO, codegen-units or strip); `release.yml` ships the editor and player but not `axcenity-server` or `axcenity-test`; no signing or notarization.

**Stale docs [V]**
- `docs/development.md:39` says "CI is off", but `.github/workflows/ci.yml` has push and PR triggers. Either Actions is disabled in the repo settings or the doc is stale. **Answered 2026-11-10:** Actions is disabled on purpose ([development policy](../development-policy.md#continuous-integration)).
- `docs/architecture/frame-loop.md:13` says editor Play has no network step, but `editor/src/play.rs:547` receives over loopback.
- `docs/architecture/crates.md:30` leaves `net` out of the editor's dependencies.

### 2.5 Strengths to preserve

- `render` knows nothing about Instances; it takes plain data keyed by `u64`. This is already the Godot servers boundary.
- Generational IDs that stay valid across the network, with Lua holding only IDs.
- Deferred change events with a 10-round cap: the same model Roblox moved to, for the same reasons [F].
- Deterministic ordering (`BTreeMap` bodies, fixed 60 Hz substeps, no scripts between substeps), and rayon planning that produces the same output on any thread count (tested).
- A strong allowlist sandbox and a watchdog that yields.
- Quantized, budgeted, prioritized motion replication with server-side movement checks.
- Diff-friendly, deterministic JSON project files; atomic saves; versioned migrations with automatic backups; one damaged file never blocks a load.
- Editor Play isolation (cloned world, `catch_unwind`), multi-client Play over the real network code, and existing headless hosts (`server`, `TestGame`) that can seed a shared simulation core.
- CI that enforces layering (the "player contains no editor code" gate), hakari, and dev-profile opt-levels already tuned for heavy crates.

---

## 3. Phase 2: Benchmark against Unreal, Godot, Unity and Roblox

Peer-engine facts carry their provenance markers. The full fact sheet with every URL is in the appendix.

### 3.1 Scene hierarchy and ECS

| Dimension | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| Object model | One `Instance` type, `ClassName` enum, parent/children IDs [V] | UObject → Actor + Components, reflected by UHT [K] | `Node` tree in `SceneTree`, composition, explicitly not ECS [K] | GameObject + Components; DOTS Entities alongside [F] | `Instance` tree under `DataModel` [K] |
| Property storage | `BTreeMap<&str, Value>` per Instance [V] | C++ fields with static reflected offsets [K] | C++ fields; ClassDB maps names to accessors; `Variant` at the API edge [K] | C# fields; DOTS: 16 KiB SoA chunks, ≤128 entities [F] | Typed reflected properties, interned names [K] |
| "Find all X" | Full tree walk, allocating [V] | `TActorIterator`; Mass queries over archetype chunks [F] | Groups [K] | `FindObjectsByType`; `EntityQuery` [K] | `CollectionService:GetTagged` (indexed) [K] |
| Change notification | Deferred ChangeLog, 10-round cap [V] | Delegates, RepNotify [K] | Signals, synchronous by default [K] | C# events [K] | Deferred mode, depth 10 [F] |
| Back-end split | Instance-free `DrawList`; physics maps `InstanceId → Body` [V] | Game-thread proxies → render-thread `FPrimitiveSceneProxy` [K] | Nodes hold RIDs into Rendering/Physics servers; servers may run async [F] | Internal [K] | Internal [K] |
| Structural change safety | Serial `&mut World`, no command buffer [V] | Mass command buffer flushed at batch end [F] | Scene tree not thread-safe; `call_deferred` [F] | `EntityCommandBuffer` [K] | Writes serial; Parallel Luau "compute parallel, write serial" [F] |

**Verdict.** At parity with Roblox at the object-model level, and ahead of Roblox's published model on renderer separation. Behind every peer on property storage, indexing and change granularity. **Best fit:** keep Roblox's DataModel as the user-facing model; store properties the way Godot's ClassDB/StringName does (per-class slots, names resolved once); complete Godot's servers pattern by pushing changes to back-ends instead of pulling the tree. **Do not convert the tree to an ECS:** Unity's Entities docs show fragmentation for heterogeneous content (100k single-entity archetypes cost over 1.5 GB) [F], which is exactly what Roblox-style games produce.

### 3.2 Asset management and streaming

| Dimension | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| Level format | One `world.json` per level, deterministic writer [V] | Binary packages; One File Per Actor with World Partition [F] | `.tscn`/`.tres` text [K] | YAML scenes [K] | `.rbxl`/`.rbxlx` [K] |
| Asset identity | Path strings; ObjectValue refs are name paths [V] | Package paths, soft refs, redirectors [K] | `uid://` + `.import` sidecar, path fallback [K] | `.meta` GUID [K] | Cloud asset IDs [K] |
| Import / cook | None; FBX/glTF/PNG parsed at runtime; raw sources in uncompressed `.axgame` [V] | Cook to platform formats, IoStore [K] | Import to `.godot/imported/` [K] | Import to `Library/`, AssetBundles [K] | Server-side processing [K] |
| Texture formats | RGBA8, CPU mips at runtime [V] | BCn/ASTC at cook [K] | VRAM-compressed at import [K] | Platform-compressed at import [K] | Compressed [K] |
| Async loading | None public; sync under global mutex [V] | Soft refs + `FStreamableManager`, Zen loader [K] | `ResourceLoader.load_threaded_request/get_status/get` [S] | Addressables handles [K] | `ContentProvider:PreloadAsync` [K] |
| World streaming | Roblox-style units + interest radius over one tree [V] | World Partition grid, Data Layers, HLOD [F] | None built-in [K] | SubScenes [K] | StreamingEnabled: target radius 1024, min 64, Atomic/Persistent models [F] |
| Hot reload | 500 ms polling watcher, last-good fallback [V] | Reimport; Live Coding [K] | Reimport; script reload into running game [K] | Reimport; domain reload [K] | Live edits [K] |

**Verdict.** Ahead on git-friendly text formats and migrations; at parity with Roblox on streaming semantics. **Significantly behind on the pipeline itself:** no import step, no stable IDs, no compressed textures, no async API. **Best fit:** Godot's `.import` sidecars with `uid://` and cached engine-native output (Godot also uses ufbx for FBX [K]), Godot's `load_threaded_*` API shape for the runtime, exposed to Luau as Roblox's `ContentProvider:PreloadAsync`. Keep StreamingEnabled semantics; don't adopt World Partition wholesale.

### 3.3 Runtime performance and execution

| Dimension | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| Threading | Main thread, fixed order; rayon inside `render` only [V] | Game, Render, RHI threads + workers [K] | Main + optional render and physics threads [F] | Main + render thread + job workers [K] | Task scheduler + Parallel Luau [F] |
| Task system | None [V] | `UE::Tasks` with prerequisites, nested tasks, Pipes [F] | `WorkerThreadPool` [K] | Job System + safety checks [K] | Internal [F] |
| Frame stages | Hand-ordered in 4 hosts, drifted [V] | Tick groups [K] | `_process` / `_physics_process` [K] | Customisable `PlayerLoop` [K] | PreRender, PreAnimation, PreSimulation, PostSimulation, Heartbeat [F] |
| Sim → render | Full rebuild every frame [V] | Proxies updated on dirty [K] | Nodes push to RenderingServer [F] | Persistent GPU Resident Drawer [F] | Internal [K] |
| Render architecture | Hand-sequenced passes, no render graph [V] | RDG: transient aliasing, pass culling, auto barriers, async compute [F] | RenderingDevice graph [K] | Render Graph, mandatory in URP 6 [F] | Internal [K] |
| GPU-driven | CPU-planned instancing; no indirect draws [V] | Nanite, ISM/HISM [K] | MultiMesh, Forward+ [F] | GPU Resident Drawer / BRG [F] | Auto instancing [K] |
| Scripting VM | Luau, no codegen, userdata `Vector3`, new userdata per access [V] | Blueprint VM, C++ [K] | GDScript typed opcodes [K] | C# + Burst [K] | Luau + `--!native` codegen, native `vector` [F] |
| Script parallelism | None [V] | Tasks/Pipes [F] | Threads + `call_deferred` [F] | Jobs [K] | Actors with serial/parallel phases and per-API safety levels [F] |

**Verdict.** The renderer reaches roughly Godot Forward+ parity in the CPU-planned style, and frame order and fixed-step physics are at parity. Behind on: no schedule or task graph, pull-every-frame back-ends, Luau without its speed features, and single-threaded simulation. A render graph is a maintainability gap rather than a hotspot: draw recording costs about 1.2 ms per the perf doc. **Best fit:** Roblox's RunService stage names as the schedule, held as data the way Unity's PlayerLoop is; Unreal Pipes / Parallel Luau's "compute in parallel, write serially" contract using rayon over `&World` (Rust's borrow checker gives Unity's job-safety guarantee for free); Unity's GPU Resident Drawer shape for rendering later.

### 3.4 Developer usability and extensibility

| Dimension | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| Scripting | Luau, sandboxed, luau-lsp definitions shipped [V] | C++, Blueprints [K] | GDScript, C#, GDExtension [K]/[F] | C# [K] | Luau with types and codegen [F] |
| Visual scripting | Blocks ↔ Lua round trip; node graphs, UI graphs, behavior trees [V] | Blueprints [K] | (removed in 4) [K] | Visual Scripting [K] | None first-party [K] |
| Editor extension | Lua plugins: toolbar, dock widgets, settings, files, Selection, undo [V] | C++ modules, Editor Utility Widgets, Python [K] | `EditorPlugin`, `@tool`, GDExtension [K]/[F] | `EditorWindow`, `PropertyDrawer` [K] | Luau plugins [K] |
| Native extension | None [V] | `.uplugin` modules [K] | GDExtension (ABI changes per minor version) [F] | UPM + asmdef [K] | None [K] |
| Code modularity | 25 crates, but `editor` 162k LOC and `script` → 11 crates [V] | Modules with Build.cs deps [K] | Monolithic C++ + modules [K] | Assembly definitions [K] | n/a |
| Iteration | Editor incremental ~5–6 s; subsecond hot-patch in debug [V] | Live Coding [F] | Script hot reload [K] | Domain reload options [K] | Instant Play [K] |
| Collaboration | Git + text files + local version history [V] | Source control + OFPA [F] | Git-friendly text [K] | Git + YAML [K] | Team Create [K] |

**Verdict.** Ahead of Roblox for its audience: plain project folders, VS Code with luau-lsp, a blocks ↔ Lua round trip, strong CI, and multi-client Play. Behind on native extension, god-crates and the egui/wgpu version lock. **Best fit:** split crates internally the way Unity's assembly definitions do, then extend the Roblox Studio plugin API toward Godot's `EditorPlugin` surface (property editors, importers, gizmos). A stable native ABI is premature.

### 3.5 Biggest gaps, ranked by impact

1. No class/tag index; allocating tree walks every frame.
2. Full per-frame draw-list rebuild.
3. Synchronous asset loading under one global mutex; no preload in the player.
4. No shared schedule; four drifting host loops.
5. String-keyed properties plus per-access Lua `String` and userdata allocation.
6. Coarse physics sync: global structure version, self-induced revisions, `wake_up_all`.
7. No asset pipeline: no stable IDs, no cook, RGBA8 and CPU mips, raw uncompressed `.axgame`.
8. Luau without its performance levers: no codegen, no native `vector`, per-fire coroutines, O(waiting) scheduler.
9. No native extension point; god-crates.
10. No GPU-driven path or render graph; egui/wgpu version lock.

Also notable: JSON replication wire and unsecured netcode; the navmesh rebuild hitch; CI without perf tracking or real GPU runs.

---

## 4. Phase 3: Package recommendations

Actions: **Add**, **Alter** (version, features or usage), **Swap** (replace a mechanism), **Remove**, **Keep**, **Reject**. Every new dependency was checked against the repo rules: pinned once in the workspace, Apache-2.0-compatible licence, cross-platform, never pulls editor code into the player.

### 4.1 Add

| Crate (pin) | Purpose | Fixes | Licence | Risk |
|---|---|---|---|---|
| `profiling = "1.0"` (1.0.18, already in the lock) | `profiling::scope!` per frame step and sub-phase | No frame timing anywhere | MIT OR Apache-2.0 [cr] | Very low: compiles to nothing without a backend |
| `tracy-client = "0.19"` | Optional `profile-tracy` feature on player/editor/server; shows rayon, texture workers and upload threads on one timeline | Same | MIT/Apache-2.0 [cr] | Low. Never on by default; never in shipped players |
| `criterion = "0.8"` (dev-only) | Micro-benchmarks: `World::property`, `descendants`, Lua `__index`, `take_due`, `Replicator::frame` | No simulation benchmarks | Apache-2.0 OR MIT [cr] | Low |
| `notify = "8.2"` (+ evaluate `notify-debouncer-full` 0.7) | Event-based file watching with `PollWatcher` fallback | 500 ms polling walk, up to 3 watchers (`project/src/watch.rs:24-27`) | **CC0-1.0** [cr] (compatible; no patent grant, worth a note) | Low–medium: network drives and WSL lose events, hence the fallback |
| `zstd = "0.14"` (or `lz4_flex` 0.14 for pure Rust) | Per-entry `.axgame` compression | Uncompressed packs unpacked to temp at launch (`project/src/packed.rs`) | BSD-3-Clause / MIT [cr] | Low. Measure the ratio first: PNG and JPEG are already compressed |
| `postcard = "1.1"` | Binary wire for reliable updates, snapshots and RemoteEvent payloads; JSON kept behind a debug flag | JSON wire (`net/src/handshake.rs:118`, `wire.rs`) | MIT OR Apache-2.0 [cr] | Medium: protocol version bump |
| `intel_tex_2 = "0.5"` (editor/cook only) | BC7/BC6H encoding at import | RGBA8 only (`render/src/renderer/gpu.rs:967-1000`) | MIT/Apache-2.0 [cr] | Medium: check Windows and Linux ispc prebuilts (macOS arm64 is secondary) [verify]; add a gate to the local platform check scripts that it never reaches the player |
| `ktx2 = "0.5"` | Cooked texture container reader in `assets` | Same | Apache-2.0 [cr] | Low. Needs wgpu `TEXTURE_COMPRESSION_BC` (desktop fine) |
| `smallvec = "1"` (already transitive) | Cached draw items per Instance; event handler snapshots | Per-frame `Vec` churn | MIT OR Apache-2.0 | Low. Don't use it for `children` before measuring |

### 4.2 Alter

| Item | Change | Why | Risk |
|---|---|---|---|
| `rust-toolchain.toml` | Pin `channel = "1.97.0"` (≥ 1.95 needed for the graphics upgrade) | CI can break on new lints with no code change | Low |
| `[profile.release]` | `lto = "thin"`, `debug = "line-tables-only"`; new `[profile.dist]` with fat LTO, `codegen-units = 1`, `strip = "debuginfo"` for `release.yml`. Keep `panic = "unwind"`: asset loaders and editor Play rely on `catch_unwind` | No release tuning today | Low; longer release CI |
| `[profile.dev.package.axcenity-script]` | `opt-level = 1` | Editor Play runs Lua glue unoptimized | Low |
| `rapier3d` | `features = ["parallel"]` behind an `axcenity-physics/parallel` feature, **after measuring** | Single-threaded solver | Medium: gains need many islands; check results stay consistent on one machine across thread counts [verify]. Cross-machine determinism is not a requirement (policy). `simd8`: defer. `enhanced-determinism`: reject |
| `mlua` | Keep 0.12.2 (latest). Later add `luau-jit` for opt-in `--!native` scripts | No codegen | Medium: x86-64/AArch64 only; check watchdog and debugger interaction [verify] |
| `luau0-src` | Keep `=0.22.0`; add a test that loads a compiled chunk and a CI check that the pin hasn't moved; file an upstream issue on mlua-sys's range | Fragile coupling | None |
| `rustc-hash` (usage) | Use `FxHashMap` for engine-keyed long-lived maps (`World` side tables `world.rs:272-275`, renderer caches `renderer/types.rs:926-931`, `scene/src/streaming.rs:46-48`); keep SipHash for network or user keys | Mixed hashing | Low |
| `rayon` (usage) | Widen to draw-list build, animator posing, audio diff and nav fingerprint over `&World`, applying writes in index order | Only `render` is parallel | Determinism handled by ordered apply |
| `renet_netcode` config | **Deferred (policy):** `ServerAuthentication::Secure` with connect tokens for internet-hosted dedicated servers; LAN and editor hosting can stay unsecure with a visible label | Unsecure on both ends (`net/src/transport.rs:240, 415`) | Medium: token issuance UX |
| Graphics stack (together) | `wgpu 30` + `eframe/egui 0.36.2` + `egui_dock 0.21.1` + `egui-phosphor 0.14` + `glyphon 0.12`; winit stays 0.30.13 [cr]. Not eframe 0.37 until egui_dock supports it | Five wgpu majors behind | **High churn** in `render` and shaders; do it only once the GPU tests pass locally on a real GPU on Windows and Linux, before and after the upgrade (CI is off by policy) |
| Patch bumps | `glam` 0.34.3, `kira` 0.12.6, `renet` 2.0.1, then `cargo hakari generate` | Hygiene | Low. The 5 glam copies come from rapier, kira and nalgebra and can only unify by following rapier |

### 4.3 Swap

| From | To | Why |
|---|---|---|
| Polling `project::Watcher` | `notify` events, keeping the stat-diff and 100 ms settle logic | Lower latency, no background stat walk |
| JSON net wire | `postcard` | Size and CPU; multi-MB snapshots against a 5 MiB channel |
| Runtime texture decode + CPU mips | Import-time BC7 + KTX2 + offline mips (with the import pipeline) | VRAM roughly 4× lower for BC7 vs RGBA8 [K]; no decode hitch |
| Fresh userdata per Instance access | Per-VM weak-valued cache `InstanceId → userdata` | Fixes the table-key bug and cuts allocations |

### 4.4 Remove (dependency edges, not packages)

- **`script → render`**: move `GraphicsSettings`, `Capabilities` and `Camera` data to `core` or a small `render-types` crate.
- **`script → audio`**: move `SoundAction` and the enum lists to `core`.
- **`script → mesh_edit`**: put CSG behind a `csg` feature.

Result: `axcenity-server` stops linking wgpu, cpal/ALSA, image and meshopt [I: confirm with `cargo tree -p axcenity-server`], and a `render` edit stops recompiling `script`, `net`, `server` and `player`.

### 4.5 Evaluated and rejected

| Candidate | Reason |
|---|---|
| `bevy_ecs` (as storage or scheduler) | Its scheduler only orders systems over data in a Bevy `World`. Using it means either moving the Instance tree into it (it becomes the real world model) or wrapping everything in one `NonSend` resource (no parallelism gained). Breaks on every quarterly release. A ~150-line custom `Schedule` covers the need |
| `avian3d` | A Bevy plugin: needs `bevy_ecs`/`bevy_app`, adds a second world model |
| Jolt bindings (`jolt_rs` etc.) | No mature maintained binding; adds C++ build and FFI surface against `unsafe_code = "deny"`. Revisit only if rapier is proven the bottleneck after the sync overhaul |
| `bincode` | Unmaintained (RUSTSEC-2025-0141) |
| `slotmap` / `thunderdome` | Neither supports the replica ID space above 2³¹ with network-stable `insert_at`; the custom arena is fine (add `iter()` and an O(1) `insert_at`) |
| `ustr` / `string-interner` for property names | A generated static `PropertyId` enum is better; a global interner leaks and is process-wide (bad for multiple editor Worlds) |
| `parking_lot` | The `Assets` problem is holding the lock during decode, not lock speed |
| `ahash` / `foldhash` | Nothing over `rustc-hash` for `u32`-pair keys |
| `puffin`, `divan`, `basis-universal`, `texpresso` | Ties egui versions again / slower cadence / stale and C++ / no BC7 |
| Separate wgpu device for the editor viewport | No portable cross-device texture sharing; keep lock-step upgrades |
| `panic = "abort"` | Breaks `catch_unwind` isolation |

---

## 5. Phase 3: Patterns to port

Five patterns, ordered by leverage per cost. Each builds on primitives Axcenity already has (`revision`, `structure_version`, `ChangeLog`, `ItemChunk`, the `DrawList` boundary), so none of them needs an ECS rewrite.

### 5.1 Data-driven frame Schedule (Roblox RunService stages + Unity PlayerLoop)

**Sources.** Roblox task scheduler stages [F] ([task scheduler](https://create.roblox.com/docs/en-us/performance-optimization/microprofiler/task-scheduler.md)); Unity `PlayerLoop` [K] ([PlayerLoop](https://docs.unity3d.com/ScriptReference/LowLevel.PlayerLoop.html)).

**Why it fits.** It replaces four hand-ordered loops that have already drifted, uses stage names creators already know, and is the prerequisite for per-stage profiling, stage parallelism (5.5) and moving gameplay systems out of `script`.

```rust
// crates/sim (new): Instance-aware, no render or egui dependencies.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Stage {
    NetReceive, Input, PreSimulation, Simulation, PostSimulation,
    Heartbeat, PreAnimation, Animation, PreRender, NetSend,
}

/// Which hosts run a step (a tiny hand-rolled bit set; no new dependency).
#[derive(Copy, Clone)]
pub struct Hosts(u8);
impl Hosts {
    pub const PLAYER: Hosts = Hosts(1);
    pub const EDITOR_PLAY: Hosts = Hosts(2);
    pub const SERVER: Hosts = Hosts(4);
    pub const TEST: Hosts = Hosts(8);
    pub const ALL: Hosts = Hosts(15);
    pub fn contains(self, other: Hosts) -> bool { self.0 & other.0 == other.0 }
}

pub struct Sim {
    pub world: World,
    pub scripts: ScriptRuntime,
    pub physics: Physics,
    pub characters: Controllers,
    pub agents: Agents,
    pub animators: Animators,
}

/// What differs per host: input source, sound sink, network.
pub struct FrameCtx<'h> { pub dt: f32, pub host: Hosts, pub io: &'h mut dyn HostIo }

#[derive(Clone)]
pub struct Step {
    pub name: &'static str,
    pub stage: Stage,
    pub hosts: Hosts,
    pub run: fn(&mut Sim, &mut FrameCtx),
}

pub struct Schedule { steps: Vec<Step> }

impl Schedule {
    /// The one order, transcribed from today's player loop.
    pub fn standard() -> Self { Self { steps: crate::steps::STANDARD.to_vec() } }

    pub fn insert_after(&mut self, anchor: &str, step: Step) {
        let at = self.steps.iter().position(|s| s.name == anchor)
            .unwrap_or_else(|| panic!("no step named {anchor}"));
        self.steps.insert(at + 1, step);
    }

    pub fn run(&mut self, sim: &mut Sim, ctx: &mut FrameCtx) {
        for step in self.steps.iter().filter(|s| s.hosts.contains(ctx.host)) {
            profiling::scope!(step.name);
            (step.run)(sim, ctx);
        }
    }
}
```

**Changes.** New `crates/sim`; the four host loops become thin hosts; the duplicated overlay UI assembly becomes one `scene::overlay_items` function; `docs/architecture/frame-loop.md` gains a generated table.

**Migration with tests green.**
1. Add profiling spans to the current loops (I1) for a before-picture.
2. Write `STANDARD` by transcribing the player's order. Add a golden test that asserts each host's step names *as each host behaves today*, with explicit per-host exceptions for the editor drift.
3. Port `TestGame` first (smallest, fixed dt), then the server, then the player, one PR each, with all game tests green.
4. Port editor Play last. Then remove the drift exceptions one at a time, each as a deliberate behaviour change with its own test.

### 5.2 Retained draw items (Godot servers + Unreal scene proxies)

**Sources.** Godot nodes hold RIDs and push to RenderingServer; using servers directly helps at "tens of thousands" of objects [F] ([using servers](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)). Unreal primitive scene proxies updated on dirty [K].

**Why it fits.** `render` stays Instance-free: the cache lives in `scene`, which already depends on `core`. `ItemChunk` is already the retained-group template.

```rust
// crates/scene/src/retained.rs
pub struct DrawCache {
    entries: FxHashMap<InstanceId, Cached>,
    order: Vec<InstanceId>,      // deterministic emit order, rebuilt on membership change
    structure_version: u64,      // ancestor flags depend on tree shape
    assets_generation: u64,      // live reload and material edits bump this
    meshes: FxHashMap<(u64, usize), Arc<ModelMesh>>, // keeps LOD OnceLocks across frames
}

struct Cached { revision: u64, items: SmallVec<[DrawItem; 1]> }

impl DrawCache {
    pub fn build(&mut self, world: &World, assets: &Assets, list: &mut DrawList) {
        if world.structure_version() != self.structure_version
            || assets.generation() != self.assets_generation
        {
            self.refresh_membership(world, assets); // uses the class index (5.3)
        }
        for id in &self.order {
            let Some(inst) = world.get(*id) else { continue };
            let cached = self.entries.get_mut(id).expect("membership is current");
            if cached.revision != inst.revision() {
                cached.items.clear();
                emit_instance(world, assets, *id, &mut self.meshes, &mut cached.items);
                cached.revision = inst.revision();
            }
            list.items.extend(cached.items.iter().cloned());
        }
    }
}
```

**Pitfalls to encode as tests.** A MeshPart's SurfaceAppearance *child* changing doesn't bump the parent's revision, so appearance-child changes must mark the parent. Streaming hidden-set changes, time-of-day-driven items and character decals must also invalidate.

**Migration.**
1. Equivalence test: on randomized worlds plus edits, `DrawCache::build` output equals `build_draw_list` output (sorted by `draw_id`).
2. Ship behind a flag, measured by the existing `bench_10000_parts` and `bench_2000_mesh_parts`.
3. Put static anchored parts into spatial `ItemChunk`s so whole cells skip LOD resolve.
4. Remove the old path.

### 5.3 Class/tag index, then per-class property slots (Godot ClassDB/StringName, Unreal FProperty)

**Sources.** Godot ClassDB and interned `StringName` [K] ([Object class](https://docs.godotengine.org/en/stable/engine_details/architecture/object_class.html)); Unreal reflection [K] ([reflection system](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-engine-reflection-system)); Roblox `GetTagged` [K].

**Why it fits.** It keeps the dynamic Roblox surface (`Value` stays, JSON stays name-keyed) but removes the walks, the string compares, most of the ~2 KB per Part, and the Lua `String` per access. There are about 400 distinct property names across about 130 classes in `core/src/class.rs` [V], so a static enum is a realistic size.

```rust
// Step 1 (M2): core/src/index.rs, maintained in World::create/set_parent/destroy/add_tag/remove_tag
pub struct Index {
    by_class: Vec<BTreeSet<InstanceId>>,          // BTreeSet keeps deterministic order
    by_tag: HashMap<Box<str>, BTreeSet<InstanceId>>,
}
impl World {
    pub fn all_of(&self, class: ClassName) -> impl Iterator<Item = InstanceId> + '_ { /* … */ }
    pub fn tagged_iter(&self, tag: &str) -> impl Iterator<Item = InstanceId> + '_ { /* O(k) */ }
    pub fn descendants_iter(&self, id: InstanceId) -> impl Iterator<Item = InstanceId> + '_ { /* no Vec */ }
}

// Step 2 (L1): core/src/prop.rs, generated by macro_rules! next to class.rs
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(u16)]
pub enum Prop { Anchored, CastShadow, Color, Material, Orientation, Position, Size, Transparency, /* ~400 */ }

impl Prop {
    pub const fn name(self) -> &'static str { NAMES[self as usize] }
    pub fn from_name(s: &str) -> Option<Prop> {
        NAMES_SORTED.binary_search_by(|(n, _)| n.cmp(&s)).ok().map(|i| NAMES_SORTED[i].1)
    }
}

pub struct Instance {
    class: ClassName,
    // name, parent, children, attributes, tags, prefab as today
    values: Box<[Value]>, // one slot per property in the class layout
    dirty: u64,           // a bit per slot since the last consumer reset
    revision: u64,
}

impl Instance {
    pub fn get(&self, p: Prop) -> Option<&Value> {
        self.class.layout().slot(p).map(|i| &self.values[i])
    }
    /// Kept as a shim so ~500 call sites can migrate gradually.
    pub fn property(&self, name: &str) -> Option<&Value> {
        Prop::from_name(name).and_then(|p| self.get(p))
    }
}
```

The `dirty` bits also give 5.2 a motion-only fast path (Position or Orientation changed → patch the transform only) and give replication a separate motion revision.

**Migration.** The index first (additive, no API break, removes the walks; add a debug-assert cross-check against a walk). Then slots behind the unchanged `property(&str)` shim, with a round-trip test for every class × every default. Then migrate hot paths (scene, physics, net) to `get(Prop::X)`. In `script/src/instance.rs`, `__index` takes `mlua::String` and borrows it with `to_str()` instead of allocating.

### 5.4 Import pipeline and threaded loading (Godot `.import` + `uid://` + `load_threaded_*`)

**Sources.** Godot import process [K] ([import process](https://docs.godotengine.org/en/stable/tutorials/assets_pipeline/import_process.html)); background loading [S] ([background loading](https://docs.godotengine.org/en/latest/tutorials/io/background_loading.html)); Roblox `ContentProvider:PreloadAsync` as the Luau-facing name [K].

**Why it fits.** It removes the main-thread decode under the global lock, gives the player a preload, and creates a home for meshopt LODs, BC7 and mips, while keeping the plain-folder project (sources stay in place; outputs go to `.axcenity/imported/`, which is already per-machine).

```rust
// Phase 1 (M5): non-blocking API; decoding happens outside the mutex
pub enum Slot<T> { Loading(LoadTicket), Ready(Arc<T>), Failed(Arc<str>) }

impl Assets {
    /// Inserts Loading under the lock, releases it, decodes on the worker pool.
    pub fn request_model(&self, key: &str) -> LoadTicket;
    /// Never blocks; the draw list skips or uses a placeholder until Ready.
    pub fn model_ready(&self, key: &str) -> Option<Arc<Model>>;
    /// Physics hulls and tests: waits on the ticket, not on the lock.
    pub fn model_blocking(&self, key: &str) -> Option<Arc<Model>>;
    pub fn progress(&self) -> (usize, usize);
    /// Bumps when anything becomes Ready; feeds DrawCache.
    pub fn generation(&self) -> u64;
}

// Phase 2 (L2): sidecar next to the source, e.g. models/ship.glb.import.json
#[derive(Serialize, Deserialize)]
pub struct ImportMeta {
    pub uid: AssetUid,          // u128, written as "ax://<base32>"
    pub importer: Importer,
    pub params: ImportParams,   // lods, bc7, max_size, srgb
    pub source_hash: u64,
}
```

**Migration.** Phase 1 is invisible to content: `model()` keeps its blocking meaning as `model_blocking`, and call sites flip one at a time. A timing-free test asserts that two concurrent loads don't serialize. Phase 2 creates sidecars lazily on first import (missing sidecar = path fallback), so existing projects load unchanged; a manifest migration is only needed when references switch to UIDs.

### 5.5 Parallel read, serial write (Roblox Parallel Luau + Unreal Pipes, via rayon)

**Sources.** Parallel Luau: Instance writes must happen in the serial phase [F] ([multithreading](https://create.roblox.com/docs/en-us/scripting/multithreading.md)); Unreal Pipes serialize access to one resource [F] ([Tasks System](https://dev.epicgames.com/documentation/en-us/unreal-engine/tasks-systems-in-unreal-engine)).

**Why it fits.** Rust's `&World` / `&mut World` split already enforces the contract. Add a step variant that computes on rayon over `&World`, returns a list of writes, and applies them serially in index order, so output stays deterministic (the renderer already proves this with its "same however many threads" test). First users: draw-list emit per chunk, animator posing for many characters, the audio sync diff, the nav fingerprint. Not Parallel Luau Actors yet: those need several `Lua` states, which strains the one-runtime rule and the sandbox. **Do this last**, once I1 numbers pick the targets and 5.1–5.3 exist.

**Considered and not picked:** an ECS or Mass layer for the Instance tree (fragmentation evidence above); a render graph now (a maintainability gain, not a hotspot at 1.2 ms of recording); GDExtension-style native plugins (premature before the crate splits).

---

## 6. Refactoring roadmap

Effort is for one developer: **S** ≤ 3 days, **M** 1–3 weeks, **L** 1–3 months. The standard measurement scene, **S10k**, is 10k parts (9k anchored, 1k unanchored falling), 200 scripts with Heartbeat and `task.wait` loops, 50 Sounds, 20 animated characters and 10 nav agents, plus the existing render benches.

### 6.1 Tier 1: Immediate, low-hanging fruit (days)

| # | Item | Files | Effort | Benefit / metric | Risk | Verdict |
|---|---|---|---|---|---|---|
| **I1** | **Measure first:** headless S10k benchmark, `profiling` spans in all four hosts and in `build_draw_list`, `ScriptRuntime::step`, `Physics::step` (sync / substeps / write-back), `Replicator::frame`; `profile-tracy` feature; criterion micro-benches | new `testing/benches/sim.rs`, hosts, workspace `Cargo.toml` | M− (5–7 d) | Turns every [I] in this report into numbers; regression baseline | Low (run locally; no nightly CI, per policy) | **Do first; gates every perf item** |
| **I2** | **Fix Instances as Lua table keys.** Test first (`t[workspace.Baseplate] = 1`, `rawequal`, `data[player]` across `PlayerAdded`), then route every `LuaInstance` creation through a per-VM weak-valued cache | `script/src/types.rs:954-958`, `instance.rs:150, 246` | S (1–2 d) | Correctness for a common Roblox idiom; fewer allocations | Low (generational IDs make reuse safe) | **Do now** |
| I3 | Fix stale docs: "CI is off", editor Play network step and its three ordering differences, `net` in the editor's deps | `docs/development.md:39`, `docs/architecture/frame-loop.md:13`, `docs/architecture/crates.md:30` | S (hours) | Accurate docs for future work | None | Do now |
| I4 | Cheap `with_world`: hold `Option<World>` in app data, or a service-less `World::placeholder()` | `script/src/lib.rs:2371-2378` | S (≤1 d) | Tens of µs per frame [I] | Low | Do |
| I5 | Toolchain pin + release/dist profiles (section 4.2) | `rust-toolchain.toml`, `Cargo.toml`, `release.yml` | S | CI stops breaking on new lints; ~5–15% CPU from thin LTO [I] | Low | Do |
| I6 | Luau pin guard test + patch bumps + `cargo hakari generate` | `script` tests, CI, `Cargo.lock` | S | Prevents silent script-loading breakage | Low | Do |
| I7 | Nav hitch: fingerprint anchored geometry only, no `format!("{:?}")` hashing | `nav/src/lib.rs:410-455` | S (1–2 d) | Removes a ~50 ms rebuild every 0.5 s when props move [I]; metric: max frame time on S10k | Agents won't path around moving unanchored props (already unreliable) | Do |
| I8 | Cache `Arc<ModelMesh>` per (model, primitive) across frames; reuse `Skin` buffers | `scene/src/draw_list.rs:1120, 1372-1380` | S–M (2–4 d) | `bench_2000_mesh_parts` build ms | Low; invalidate on reload | Do (stepping stone to M4) |
| I9 | Non-allocating `descendants_iter()`; `FxHashMap` for engine-keyed maps | `core/src/world.rs:1194-1208, 272-275`, `render/src/renderer/types.rs:926-931`, `scene/src/streaming.rs:46-48` | S–M (3–5 d) | Fewer allocations; shrinks M2's diff | Low | Do |
| I10 | `notify` watcher with poll fallback | `project/src/watch.rs` | S–M (3–4 d incl. 3-OS test) | Reload latency 0.5–0.6 s → ~0.1 s | Platform quirks | Do, low priority |
| I11 | Player background texture loading + preload screen from `scene::asset_files` | `player/src/main.rs`, `scene/src/preload.rs` | S–M (2–4 d) | First-frame and max-frame ms | Textures pop in | Do (cheap half of 5.4) |

### 6.2 Tier 2: Medium-term subsystem overhauls (weeks)

| # | Item (pattern) | Effort | Benefit / metric | Risk | Depends on | Verdict |
|---|---|---|---|---|---|---|
| **M1** | Shared `Schedule` + `sim` crate (5.1) | M (2–3 wk) | Ends host drift; spans per stage; golden step-order tests | Medium: editor Play order changes | I1 | **High value, do** |
| **M2** | Class/tag index + per-service structure version (5.3 step 1) | M (1–2 wk) | Removes ~15–20 O(N) walks/frame [I]; ms in non-render stages on S10k | Low–medium: index consistency, guarded by debug cross-check | I9 | **Highest leverage, do** |
| M3 | Physics sync overhaul: physics-owned revision, added/removed lists instead of global resync, wake neighbours not `wake_up_all`, write-back once per frame; then try `parallel` | M (2–3 wk) | Physics ms on S10k with moving bodies | Medium: vehicle and joint drift (game tests guard) | M2, I1 | Do; measure `parallel` separately |
| **M4** | Retained draw list (5.2) | M (2–4 wk) | `bench_10000_parts` build 3.3 ms → target <1 ms for static scenes [I] | Medium: stale visuals from missed invalidation | M2, I8 | **Do** |
| M5 | Assets off-lock + threaded loading API + `ContentProvider:PreloadAsync` (5.4 phase 1) | M (2 wk) | No hitch on first sight of a big FBX | Medium: placeholder policy | I11 | Do |
| M6 | Script hot paths: `mlua::String` keys, member table per class, binary-heap scheduler, prune connections on Destroy, compiled-chunk cache by source hash, thread reuse only for handlers proven not to yield | M (2–3 wk) | Script-stage ms on S10k; ns per `part.Position` | Medium: coroutine semantics vs Roblox [verify] | I1, I2 | Do, item by item |
| M7 | Replication: motion revision + `postcard` wire | M (1–2 wk) | Server ms with 8 loopback clients; snapshot bytes | Medium: protocol bump | M2 | Do |
| **M8** | Cut `script → render/audio/mesh_edit` edges (4.4) | S–M (1 wk) | Server stops linking wgpu/cpal; smaller rebuild fan-out | Low | — | **Cheap, do early** |
| M9 | `.axgame` zstd + read in place (no temp unpack) | M (1 wk) | Pack size, launch time | Low | — | Do after measuring the ratio |
| M10 | CI: lavapipe GPU tests on Linux, nightly I1 benches, run `axcenity-test` and the server headless, ship server + test binaries, `intel_tex_2` player gate | S–M (1 wk) | Real render coverage; perf trend | Low | I1 | **Deferred (policy: Actions is off).** The platform check scripts already run `axcenity-test` and the headless server locally; add the `intel_tex_2` gate there with L2. Revisit when the architecture is stable |
| M11 | renet secure mode for dedicated servers | M (1–2 wk) | Encrypted, token-gated connections | Medium: token UX | — | **Deferred (policy).** Internet hosting is a future objective; do this before any internet-hosting claim |
| M12 | wgpu 30 / egui 0.36 stack upgrade | M–L (2–4 wk) | Unblocks newer wgpu features (and L5) | **High**: API and naga churn | I5 | Do, once the GPU tests pass locally on a real GPU on Windows and Linux (replaces the old M10 gate) |
| M13 | Nav build on a worker + one shared navmesh cache (drop the second cache in `script::humanoid`) | S–M (1 wk) | No build hitch; no double builds | Low | I7 | Do |
| M14 | UI layout cache: reuse `TaffyTree`, style cache, one layout per editor frame | M (2 wk) | UI ms with a 500-element inventory | Medium | I1 | Do if I1 shows UI > 1 ms |

### 6.3 Tier 3: Long-term architectural shifts (months)

| # | Item | Effort | Benefit / metric | Risk | Depends on | Verdict |
|---|---|---|---|---|---|---|
| L1 | Per-class property slots + `Prop` IDs + dirty bits (5.3 step 2) | L (1–2 mo) | ~2 KB → ~0.5 KB per Part [I]; ns per read; cheaper `World::clone` for undo and Play | Medium–high: ~500 call sites; reflection consumers (Properties panel, world file, saves, net) | M2 | Do **after** M2 and M4 show property access still dominates |
| L2 | Import pipeline: sidecars + UIDs + cooked meshes + offline LODs + BC7/KTX2 + offline mips (5.4 phase 2) | L (1.5–2 mo) | VRAM ~4× lower for textures [K]; no runtime FBX parse in shipped games; rename-safe references | Medium: two content paths during transition | M5, M9 | **Do; the biggest shipped-game win.** Desktop formats only (BC7); ASTC/ETC2 aren't needed while mobile is deferred |
| L3 | Faster Vector3 (first-class engine type) + opt-in `luau-jit` (`--!native`) | L (1 mo+) | Numeric script benches; allocations per `part.Position + v` | **High** compatibility: `typeof`, field casing, definitions, blocks translator | M6 | `luau-jit` opt-in first (cheap; x86-64/AArch64 is fine for desktop). **Don't choose Luau's native `vector` representation until its type identity and interop are verified** against the checks in the [policy](../development-policy.md#scripting-api-and-vector3); keep a compatible public Vector3 API over whatever internal representation is fastest. Exact `typeof(v) == "Vector3"` is desirable if it doesn't compromise the architecture |
| L4 | Parallel read / serial write stages (5.5) | L (1–2 mo, incremental) | S10k frame ms on 8+ cores | Medium: ordered apply for determinism | M1, M4 | Do, targeted by I1 data |
| L5 | GPU-driven path: shared mesh arena, persistent instance buffers for static items, slim shadow stream, `multi_draw_indexed_indirect` | L (2–3 mo) | Render CPU ms on forest/terrain benches | Medium: feature availability per backend (Vulkan and DX12 first; Metal is secondary) [verify] | M12 | Later, after M4 removes the scene-side cost |
| L6 | Split `editor` into sub-crates behind an `EditorContext` (animation, model_studio, ui_designer, block_editor first, ~50k LOC) | L (2–3 mo, incremental) | Incremental rebuild 5–6 s → ~2 s target [I]; enforced boundaries | Medium: 109-field `EditorApp` | — | Do opportunistically, one panel family per PR |
| L7 | Move gameplay systems out of `script` into a `gameplay` crate behind a `GameSystem` trait (registers its Lua API and its schedule steps) | L (1–2 mo) | `ScriptRuntime` shrinks to a VM core; `net` talks to a message interface instead of ~30 methods | Medium | M1, M8 | Do after M1 proves the Schedule |
| L8 | Render graph (transient aliasing, pass culling) | L | Maintainability | Low urgency | M12 | **Defer** |

### 6.4 Dependency spine and cost-vs-benefit

```
I1 ──► I2 … I11 (in parallel)
   └─► M8, M2 ──► M1, M3, M4 ──► M5, M6, M7 ──► L1, L4, L7
local GPU tests (Windows, Linux) ──► M12 ──► L5
Deferred by policy: M10 (CI), M11 (internet hosting)
M5 + M9 ──► L2
```

- **Best ratio:** I2 (a correctness bug, 1–2 days), I1 (unblocks every decision), M2 (one index, about seven crates benefit), M8 (one week, server stops linking GPU and audio), I7 (a 50 ms hitch for about a day [I]).
- **High benefit, high cost, worth it:** M1 (correctness across four hosts), M4, L2.
- **Uncertain; measure before committing:** rapier `parallel`/`simd8`, `luau-jit`, L1 property slots (M2 + M4 may already remove most property-read volume), M14.
- **Rejected:** listed in section 4.5.

A reasonable first quarter for one developer: I1–I11 (about 4–5 weeks), then M8, M2 and M1 (about 5 weeks), then M4 (about 3 weeks). That sequence fixes the known bug, removes the largest per-frame costs, ends host drift, and leaves the engine instrumented so every later decision is made with numbers.

---

## 7. Open questions

All five were answered by Paul on 2026-11-10. The full answers and the policy that came with them are in [development-policy.md](../development-policy.md).

1. Is GitHub Actions actually disabled for the repo (as `docs/development.md` says), or is the doc stale?
   **Answer:** disabled on purpose, while the architecture changes. Testing and verification are local. CI can be reconsidered once the architecture and core systems are stable. M10 is deferred.
2. Is cross-machine determinism ever a goal (lockstep multiplayer, replays)? If yes, rapier `parallel`/`simd8` and Luau codegen need more care.
   **Answer:** not now. The priority is reliable, consistent physics behavior and performance on a single machine. Lockstep and deterministic replay are possible later and must not add complexity now, but the architecture shouldn't needlessly prevent them.
3. Are mobile or web targets planned? Luau codegen has no wasm path, and BC7 is desktop-only (ASTC would be needed).
   **Answer:** not as immediate priorities. Desktop first, with Windows and Linux as the primary platforms. Stay reasonably portable, but graphics formats and scripting favor desktop performance and capability.
4. Does the Roblox-compatible surface need `typeof(v) == "Vector3"` exactly? That decides whether native `vector` (L3) is viable.
   **Answer:** the API is Roblox-inspired, not perfectly compatible, but common Roblox-style APIs and expected type behavior are kept where reasonably practical. Vector3 is a first-class engine type; exact `typeof` is desirable if it doesn't compromise the architecture. Don't choose Luau's native `vector` until its type identity and interop are verified; a compatible public API can sit over an optimized internal representation.
5. Is internet hosting of dedicated servers planned soon? If so, M11 moves up.
   **Answer:** it's a future objective, not an immediate priority. The focus is local multiplayer, server/client architecture, networking fundamentals and testing in controlled environments. M11 is deferred.

---

## Appendix A. Peer-engine sources

Fetched and read [F]:
- Roblox: [deferred events](https://create.roblox.com/docs/en-us/scripting/events/deferred.md), [Parallel Luau](https://create.roblox.com/docs/en-us/scripting/multithreading.md), [StreamingEnabled](https://create.roblox.com/docs/en-us/workspace/streaming.md), [task scheduler](https://create.roblox.com/docs/en-us/performance-optimization/microprofiler/task-scheduler.md), [native code generation](https://create.roblox.com/docs/luau/native-code-gen)
- Godot: [using servers](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html), [thread-safe APIs](https://godot-es.readthedocs.io/es/4.x/_sources/tutorials/performance/thread_safe_apis.rst.txt), [internal rendering architecture](https://docs.godotengine.org/fr/4.5/_sources/engine_details/architecture/internal_rendering_architecture.rst.txt), [GDExtension](https://docs.godotengine.org/en/stable/engine_details/engine_api/gdextension/what_is_gdextension.html)
- Unreal: [Render Dependency Graph](https://dev.epicgames.com/documentation/en-us/unreal-engine/render-dependency-graph-in-unreal-engine), [Tasks System](https://dev.epicgames.com/documentation/en-us/unreal-engine/tasks-systems-in-unreal-engine), [Mass Entity](https://dev.epicgames.com/documentation/unreal-engine/overview-of-mass-entity-in-unreal-engine), [World Partition](https://dev.epicgames.com/documentation/en-us/unreal-engine/world-partition-in-unreal-engine)
- Unity: [Entities chunk allocations](https://docs.unity3d.com/Packages/com.unity.entities@1.4/manual/performance-chunk-allocations.html), [URP render graph](https://docs.unity.com/en-us/engine/6000.3/manual/render-pipelines/universal-render-pipeline/customizing-urp/render-graph/introduction.md), [GPU Resident Drawer](https://docs.unity3d.com/6/Documentation/Manual/urp/gpu-resident-drawer.html)

Cited from knowledge or search [K]/[S] (verify before quoting): Unreal object model, cooking, ticking, memory ([objects](https://dev.epicgames.com/documentation/en-us/unreal-engine/objects-in-unreal-engine), [actor ticking](https://dev.epicgames.com/documentation/en-us/unreal-engine/actor-ticking-in-unreal-engine), [cooking](https://dev.epicgames.com/documentation/en-us/unreal-engine/cooking-content-in-unreal-engine), [async loading](https://dev.epicgames.com/documentation/en-us/unreal-engine/asynchronous-asset-loading-in-unreal-engine)); Unity GameObjects, Jobs, Burst, PlayerLoop, Addressables, UPM, asmdef ([job system](https://docs.unity3d.com/Manual/job-system.html), [PlayerLoop](https://docs.unity3d.com/ScriptReference/LowLevel.PlayerLoop.html), [Addressables](https://docs.unity3d.com/Packages/com.unity.addressables@2.3/manual/index.html), [assembly definitions](https://docs.unity3d.com/Manual/assembly-definition-files.html)); Godot import/UID, WorkerThreadPool, MultiMesh ([import process](https://docs.godotengine.org/en/stable/tutorials/assets_pipeline/import_process.html), [WorkerThreadPool](https://docs.godotengine.org/en/stable/classes/class_workerthreadpool.html), [MultiMesh](https://docs.godotengine.org/en/stable/tutorials/performance/using_multimesh.html), [ResourceLoader](https://docs.godotengine.org/en/latest/classes/class_resourceloader.html)); Roblox Instance signals, plugins, packages, adaptive timestepping ([Instance](https://create.roblox.com/docs/reference/engine/classes/Instance), [plugins](https://create.roblox.com/docs/studio/plugins), [packages](https://create.roblox.com/docs/projects/assets/packages), [adaptive timestepping](https://create.roblox.com/docs/physics/adaptive-timestepping)).

Crate facts: crates.io API (`https://crates.io/api/v1/crates/<name>/<version>`), and [RUSTSEC-2025-0141](https://rustsec.org/advisories/RUSTSEC-2025-0141) for bincode.

## Appendix B. Working notes

The six intermediate reports this summary was built from (the three Phase 1 audits, the peer-engine fact sheet, the benchmark and the strategy) are kept beside this report in `docs/research/engine-modernization/`. They carry the full `file:line` evidence for every row above.
