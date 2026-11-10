# Phase 2: Engine benchmark — Axcenity vs Unreal 5, Godot 4, Unity 6, Roblox

Date 2026-11-10. Inputs: `phase1-runtime.md` (R), `phase1-sim.md` (S), `phase1-tools.md` (T), `phase2-engine-facts.md` (EF).
Repo `/home/user/axcenity-engine` (read-only). Axcenity paths are relative to `crates/` unless they start with `docs/`, `.github/` or `Cargo.toml`.

**Markers.** Axcenity cells: **[V]** read in code by a Phase 1 auditor (file:line given); **[V✓]** additionally spot-checked
by this benchmark today; **[I]** inference, not measured. Other-engine cells carry EF's provenance: **[F]** page fetched,
**[S]** URL found but body not read, **[K]** from knowledge, unverified. No [K] claim is upgraded here. Where an engine cell
has no marker it is a restatement of the EF claim in the same row's source section (same marker as EF).

Spot-checks performed (all held): `core/src/world.rs:55` BTreeMap properties; `scene/src/draw_list.rs:449-456` full
Workspace walk; `assets/src/lib.rs:107-109` `Arc<Mutex<Shared>>`; `physics/src/lib.rs:973-976` `wake_up_all`;
`Cargo.toml:79,84,115` mlua `luau` only / `luau0-src = "=0.22.0"` / rapier3d with no features; `script/src/instance.rs:114-121`
`key: String`; `script/src/lib.rs:2371-2378` `with_world` via `mem::take`; `script/src/lib.rs:168` `MAX_CHANGE_ROUNDS = 10`;
`core/src/world.rs:1194` `descendants -> Vec`; `project/src/watch.rs` 500 ms / 100 ms polling; `rust-toolchain.toml` `stable`;
no `[profile.release]` in `Cargo.toml`; no `multi_draw`/`draw_indexed_indirect`/BCn/ASTC and no render-graph type in `render/src`.

---

## 0. Filling the "(verify)" cells of EF's cross-engine table

| Concern | Axcenity today (verified from Phase 1) |
|---|---|
| Change notify | **Deferred, Roblox-style.** `World` records an opt-in `ChangeLog` (`core/src/world.rs:206-253, 531, 564-571`) [V]; `script::changes` turns it into `Changed`, `GetPropertyChangedSignal`, `ChildAdded`, `DescendantAdded`, `AncestryChanged` at fixed drain points (`ScriptRuntime::fire_changes`, `script/src/lib.rs:2032-2055`), with a 10-round re-entrancy cap (`lib.rs:168` [V✓]) — same depth limit as Roblox Deferred [F]. Physics writes flood it (S §3.3). |
| Async load | **Mostly synchronous.** Models/textures load lazily on first `Assets::model()`/`texture()` call, inside one global mutex (`assets/src/lib.rs:107-109` [V✓], `:236-264`). 4 texture worker threads exist but only the editor enables them (`editor/src/app.rs:1183`); the player has no preload (R §3.5). Background GPU uploads only when streaming is on (`player/src/main.rs:839-843`). No soft references, no load handles/progress API for scripts. |
| Open world | **Instance streaming, Roblox-shaped.** `core/src/streaming.rs` partitions Workspace into units; host-side hidden set ticks every 0.2 s (`scene/src/streaming.rs:93-112`); net interest management for StreamingEnabled (`net/src/streaming.rs`, S §1); terrain chunk worker thread when streaming (`scene/src/terrain.rs:125`). No cell files, HLOD, or data layers; the world is still one `world.json` per level (T §3.1). |
| Instancing | **CPU-planned instancing.** Batches by mesh+material with per-instance buffer, frustum culling for camera and every shadow view, LOD with hysteresis, `ItemChunk` retained groups for foliage, GPU skinning and particles (R §5). No indirect/multi-draw, no shared mesh arena, full instance-buffer rewrite each frame (`render/src/renderer/scene.rs:886-908`) [V; grep re-run ✓]. |

---

## 1. Pillar 1 — Scene hierarchy and ECS

### 1.1 Comparison

| Dimension | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| Core object model | One `Instance` type per node, `ClassName` enum, parent/children `Vec<InstanceId>` (`core/src/world.rs:50-65`) [V✓] | UObject → AActor + UActorComponent/USceneComponent; transform tree in SceneComponents [K] | `Node` tree in `SceneTree`; composition by child nodes, explicitly not ECS [K] | GameObject + Components (always Transform) [K]; DOTS Entities beside it [F] | `Instance` tree under `DataModel`; Services as top children [K] |
| Identity / lifetime | Generational arena `Id{index,generation}`, stale handles fail as `None`; separate ID space for replicas (`core/src/arena.rs`, `world.rs:25-47`) [V] | GC'd UObjects, reachability via UPROPERTY, incremental in 5.4 [K] | Nodes freed manually (`queue_free`), Resources refcounted, server objects as RIDs [K]/[F] | Managed objects + native side; Boehm GC for C# [K] | Instances destroyed explicitly; refs held by Luau [K] |
| Property storage | `BTreeMap<&'static str, Value>` per Instance; 16-variant `Value` (≈56 B [I]); ~28 entries per Part, ≈2–2.5 KB heap [I] (`world.rs:55` [V✓], `value.rs:175-200`, `class.rs:2015-2058`) | Fields in C++ structs, reflected by UHT metadata (static offsets) [K] | C++ fields; ClassDB maps names→getters/setters, `Variant` boxing at the API edge [K] | C# fields serialized by reflection [K]; DOTS: 16 KiB SoA chunks, ≤128 entities per chunk [F] | Typed reflected properties (internals unpublished) [K] |
| Name lookup cost | O(log P) string compares per read; Lua `__index` first copies key to `String`, then a ~9-step string chain, then linear `ClassMethods` scan (`script/src/instance.rs:114-249, 466-474`) [V✓] | Compiled field access; reflection only for tools/BP [K] | StringName (interned) lookups; GDScript typed opcodes bypass [K] | Compiled field access [K] | Interned names, inline caches in Luau VM [K] |
| Queries ("all X") | None: no class or tag index; `descendants()` allocates a full `Vec` (`world.rs:1194` [V✓]); ~15–20 full walks/frame across crates (S §3.1, R §3.2) [I count]; `tagged()` O(N) (`world.rs:1142-1160`) | TActorIterator; Mass EntityQuery over archetype chunks [F] | Groups (`get_nodes_in_group`) [K] | `FindObjectsByType`; DOTS `EntityQuery` [K] | `CollectionService:GetTagged` indexed [K] |
| Change notification | Deferred ChangeLog → events, 10-round cap (see §0) [V✓] | Delegates; RepNotify for replicated props [K] | Signals, synchronous by default; `call_deferred` [K]/[F] | C# events; no built-in property-change signal [K] | `Changed`/`GetPropertyChangedSignal`, Deferred mode recommended, depth 10 [F] |
| Change detection for systems | Per-Instance `revision`, global `structure_version` (`world.rs:62-87, 813-816`) [V]; used by physics/net, **not by scene** (R §3.2) | Dirty flags per component (render state, transforms) [K] | `NOTIFICATION_TRANSFORM_CHANGED` pushes to servers [K] | `Transform.hasChanged`; DOTS change versions per chunk [K] | Internal [K] |
| Structural change safety | Serial single-thread mutation of `&mut World`; no command buffer (R §3.8) [V] | Mass `MassCommandBuffer` flushed at batch end [F] | Scene tree not thread-safe; `call_deferred` [F] | `EntityCommandBuffer` [K] | Instance writes serial; Parallel Luau "compute parallel, write serial" [F] |
| Back-end split | Render consumes Instance-free `DrawList` with `Arc<dyn MeshSource>` keyed by `u64` (`render/src/draw_list.rs:1334-1421`) [V]; physics maps `InstanceId→Body` in a `BTreeMap` (`physics/src/lib.rs:117-125`) [V] | Game-thread proxies → render-thread `FPrimitiveSceneProxy` [K] | Nodes hold RIDs into Rendering/Physics/Audio servers; servers may be async [F] | Renderer/BRG internal [K]/[F] | Internal [K] |
| Bulk-sim layer | None | Mass Entity plugin, ISM switching by LOD [F]/[K] | None by design; servers used directly for 10k+ objects [F] | DOTS Entities + Baking [F]/[K] | None; Parallel Luau Actors [F] |
| Prefabs / instancing of subtrees | `core/src/prefab.rs`, links + overrides inline in world JSON (T §3.1) [V] | Blueprints, Level Instances [K]/[F] | PackedScene, inherited scenes [K] | Nested prefabs + variants [K] | Packages (versioned, auto-update) [K] |

### 1.2 Narrative

**Parity.** At the object-model level Axcenity is at parity with Roblox and close to Godot: a single reflected node type, names,
attributes, tags, and deferred change events with the same 10-round depth limit Roblox documents [F]. The generational arena
with Lua holding only IDs (S §4) is safer than Godot's manual `free()` and avoids Unreal's GC pauses.

**Ahead.** Clean separation of the renderer from the Instance tree (`DrawList` contract) is already Godot-server-shaped and is
ahead of Roblox's published model. Deterministic `BTreeMap` ordering in physics (`physics/src/lib.rs:117-125`) is a deliberate
reproducibility choice none of the four document as a default.

**Behind.** Three things, all in `core`:
1. Property access is runtime string lookup on a BTree of fat enums. Every peer resolves names once (UHT offsets, Godot
   `StringName`, Unity compiled fields, Roblox interned names + VM inline caches [K]); Axcenity pays it per read, per frame,
   per system — and the Lua binding adds a `String` allocation and a string-match chain on top (`instance.rs:114-249`).
2. No class/tag index. "Find all Sounds" is a full-tree walk with a `Vec` allocation; Roblox's `GetTagged`, Godot groups and
   Unity queries are indexed [K]. This is the single most repeated cost in Phase 1 (S §3.1).
3. Change tracking is coarse: one global `structure_version` (any reparent anywhere forces a full physics resync,
   `physics/src/lib.rs:782-905`) and physics' own writes bump revisions and flood the ChangeLog (S §3.2–3.3).

**Best fit to borrow.**
- **Keep Roblox's DataModel as the user-facing model** — it is the product goal, and the deferred ChangeLog is already the
  right shape (Roblox's rationale [F]: engine can assume the place only changes at resumption points).
- **Borrow Godot's ClassDB + StringName idea for storage, not Unity/Unreal ECS.** Intern property names to per-class slot
  indices at `ClassName` registration (class.rs already builds default property lists), store `Vec<Value>`/SmallVec by slot,
  and let the Lua binding resolve `key → slot` once per (class, string) via an mlua string-pointer cache. This preserves the
  dynamic, scriptable surface Roblox users expect while removing string compares — exactly what ClassDB does for `Variant`.
- **Borrow Godot's servers pattern for back-ends.** Instance → render/physics handles should be pushed on change (like
  `NOTIFICATION_TRANSFORM_CHANGED` into RenderingServer) rather than pulled by walking the tree each frame. Axcenity's
  `DrawList` is already a server boundary; adding a retained `InstanceId → DrawHandle` map keyed by `revision` completes it.
- **Borrow Mass/ECB only for a future bulk layer** (crowds, projectiles): deferred structural changes flushed at a stage
  boundary map directly onto the ChangeLog drain points. Do not convert the Instance tree to an ECS: DOTS' own docs show
  archetype fragmentation for heterogeneous prefab-heavy content (100k single-entity archetypes >1.5 GB [F]), which is
  precisely Roblox-style content.

---

## 2. Pillar 2 — Asset management and streaming pipelines

### 2.1 Comparison

| Dimension | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| Scene/level format | One `world.json` per level, nested Instances, defaults omitted, hand-rolled deterministic writer (`project/src/world_file.rs:1-67, 769`) [V] | `.umap`/`.uasset` binary packages; One File Per Actor with World Partition [K]/[F] | `.tscn`/`.tres` text (binary optional) [K] | YAML scenes/prefabs (Force Text) [K] | `.rbxl`/`.rbxlx` binary/XML [K] |
| Diff/merge friendliness | Good diffs (sorted keys); poor merges (whole level in one file) [I] | OFPA solves level checkout contention [F] | Good; per-scene files [K] | YAML merge via SmartMerge [K] | Team Create, not file merge [K] |
| Asset identity | **Path strings**; ObjectValue refs are name paths, "first sibling with same name wins" (T §3.1) [V]; no GUID/UID sidecars (grep) [V✓] | Package paths + soft object paths, redirectors [K] | `uid://` + `.import` sidecar, path fallback [K] | `.meta` GUID + fileID [K] | Numeric cloud asset IDs [K] |
| Import / cook | None: FBX/glTF/PNG parsed at runtime by ufbx/gltf/image; LODs by meshopt at load (`assets/src/model/lod.rs:99-131`) [V]; `.axgame` ships raw sources, uncompressed (`project/src/packed.rs`) [V] | Cook to platform formats; IoStore `.utoc/.ucas` [K] | Import to `.godot/imported/`, engine-native [K] | Import to `Library/`; AssetBundles [K] | Server-side processing of uploads [K] |
| Texture formats | RGBA8 only, CPU box-filter mips at runtime (`render/src/renderer/gpu.rs:967-1000`) [V; no BCn/ASTC grep ✓] | Platform compressed (BCn/ASTC) at cook [K] | VRAM-compressed at import [K] | Platform compressed at import [K] | Client-side compressed [K] |
| Async loading API | No public async API; lazy sync load under global `Mutex` (`assets/src/lib.rs:107, 236-264`) [V✓]; texture workers editor-only (`editor/src/app.rs:1183`) [V] | Soft refs + `FStreamableManager`/`AsyncLoadPackage`, Zen loader thread [K] | `ResourceLoader.load_threaded_request/get_status/get` [S] | Addressables `AsyncOperationHandle`, refcounted release [K] | `ContentProvider:PreloadAsync` [K] |
| Preload / loading screen | `scene::asset_files` exists but only the editor uses it (`editor/src/loading.rs:221`) [V] | Asset Manager bundles [K] | Threaded load + progress [S] | Addressables labels [K] | PreloadAsync + streaming pause [K]/[F] |
| World streaming | Streaming units + interest radius over the same Instance tree (`core/src/streaming.rs`, `net/src/streaming.rs`) [V]; terrain chunks on worker when streaming (`scene/src/terrain.rs:125, 393`) [V] | World Partition runtime grid, streaming sources, Data Layers, HLOD per cell [F] | None built-in [K] | SubScenes streamed async [K] | StreamingEnabled: target/min radius, Atomic/Persistent models, `RequestStreamAroundAsync` [F] |
| Hot reload | Polling watcher 500 ms + 100 ms settle (`project/src/watch.rs:24-27` [V✓]); (mtime,size) stamps with last-good fallback (`assets/src/reload.rs`) [V]; scripts reloaded in place in editor Play (`editor/src/play.rs:847-872`) [V]; player `--watch` assets only | Reimport from source; Live Coding for C++ [K] | Reimport on focus; script reload into running game [K] | Reimport; domain reload [K] | Studio edits live; Packages auto-update [K] |
| Versioning / migration | Manifest v4 migrations with automatic backups (`project/src/migrate.rs:71-78, 127`) [V] | Package versioning, core redirects [K] | Format version per file [K] | Serialized version + FormerlySerializedAs [K] | Engine-managed [K] |
| Robustness | Loader never panics (catch_unwind), failures cached once (`assets/src/model.rs:550-610`) [V] | — | — | — | — |

### 2.2 Narrative

**Parity / ahead.** World streaming is Roblox-shaped (interest management over one Instance tree, a per-client replication
policy) — the EF mapping recommends exactly this [F], and Axcenity already has it. Text-first, deterministic, VS-Code-editable
project files with atomic saves and versioned migrations are at parity with Godot and ahead of Roblox's binary places for
git workflows. Hot reload with last-good fallback is at parity with Godot.

**Behind — significantly.** Axcenity has **no asset pipeline** in the Unreal/Godot/Unity sense: no import step, no stable IDs,
no cooking, no compressed textures, no async API. Consequences already visible in Phase 1: first-sight hitches in the player
(decode on main thread during `build_draw_list`, R §3.5), a global lock that serialises model parse + meshopt behind every
other asset caller, CPU RAM ≈ GPU RAM for textures (decoded `Image` kept, R §3.5), and an uncompressed `.axgame` unpacked to
temp on every launch (T §4.8). Path-based references break on rename; name-path ObjectValues are ambiguous with duplicate
sibling names.

**Best fit to borrow.**
- **Godot's import model** (`.import` sidecar + `uid://`, cached engine-native output) is the best fit: it keeps source files
  in the project (matching Axcenity's plain-folder ethos), adds stable IDs, and gives a natural place for meshopt LODs,
  BC7/ASTC encoding and offline mips. Godot even uses ufbx for FBX [K], the same library Axcenity uses. Unity's `.meta`
  GUID is equivalent; Unreal's binary packages contradict the diff-friendly goal.
- **Godot's `ResourceLoader.load_threaded_*` API shape** [S] for the runtime: request/status/get with progress maps to a
  "loading" marker inside `Assets` and work done outside the mutex. Expose it to Luau as Roblox's
  `ContentProvider:PreloadAsync` [K] so creators get the familiar API.
- **Keep Roblox StreamingEnabled semantics** [F] (min/target radius, Atomic/Persistent models, `RequestStreamAroundAsync`) and
  add per-unit asset prefetch; do not adopt World Partition cells/OFPA wholesale. If level-file merge conflicts become a team
  problem, OFPA's idea [F] can be applied cheaply as "one file per top-level Model" without changing the runtime.

---

## 3. Pillar 3 — Runtime performance and execution

### 3.1 Comparison

| Dimension | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| Threading model | Everything on the main thread in fixed order; rayon only inside `render` (>4,096 items); ad-hoc `std::thread` for texture workers, upload thread, terrain chunk worker, CSG (R §2, S §3.10) [V] | Game, Render (≤1 frame behind), RHI threads + worker pool [K] | Main + optional separate render thread (doc warns of bugs) + optional physics thread [F] | Main + render thread + job workers [K] | Task scheduler; Parallel Luau across Actors [F] |
| Task system | None; no job graph or schedule abstraction (T §0.1) [V] | `UE::Tasks` with prerequisites, nested tasks, Pipes [F] | `WorkerThreadPool` add_task/add_group_task [K] | C# Job System + dependencies + safety checker [K] | Internal scheduler [F] |
| Frame stages | Hand-ordered in 4 hosts, already drifted (render_step vs animators, audio, particles) (T §1.4) [V] | Tick groups with dependencies [K] | `_process` / `_physics_process` [K] | Customisable `PlayerLoop` [K] | RunService PreRender/PreAnimation/PreSimulation/PostSimulation/Heartbeat [F] |
| Physics step | rapier3d 0.36, fixed 60 Hz accumulator, ≤10 steps, no `parallel`/`simd` features (`Cargo.toml:115` [V✓], `physics/src/lib.rs:93, 535`) [V]; no render interpolation found [I]; `wake_up_all` on any change (`lib.rs:975` [V✓]) | Chaos, optional async fixed step [K] | Fixed 60 Hz; Jolt option/default 4.6 [K] | Fixed timestep PhysX [K] | 240 Hz adaptive per assembly [K] |
| Sim → render handoff | Full `build_draw_list` rebuild each frame, single-threaded, ≥4 Workspace walks (`scene/src/draw_list.rs:449-529` [V✓]); 3.3 ms/10k parts, 6.1 ms/50k streamed (docs) | Proxies updated on dirty only [K] | Nodes push to RenderingServer on change [F]/[K] | Dirty transforms; GPU Resident Drawer persistent [F] | Internal [K] |
| Render architecture | Monolithic `ScenePipeline::draw` sequencing passes by hand; no render graph (grep ✓) (`render/src/renderer/scene.rs:532`) [V] | RDG: transient aliasing, pass culling, auto barriers, async compute, parallel recording [F] | RenderingDevice with acyclic graph (4.3) [K]; Forward+ clustered [F] | SRP Render Graph, mandatory in URP 6 [F] | Internal [K] |
| GPU-driven / instancing | CPU batching by mesh+material, frustum + LOD + hysteresis, GPU skinning (13×) and particles; no indirect draws, per-mesh VB/IB, full instance rewrite, per-view caster copies (R §3.4) [V] | Nanite, ISM/HISM, GPU Scene [K] | MultiMesh, Forward+ auto-instancing, CPU occlusion (Embree) [F]/[K] | GPU Resident Drawer/BRG + GPU occlusion [F] | Automatic instancing of identical parts [K] |
| Memory allocation | Global allocator; per-frame `Vec`/`Arc`/`DrawItem` churn (R §3.2–3.3); `World::clone` for undo/Play (R §3.7) [V] | FMalloc binned, mem-stack frame allocators [K] | RID_Owner chunked allocators, refcounting [K] | NativeContainer Temp/TempJob/Persistent [K] | Internal [K] |
| Scripting VM | Luau via mlua 0.12 (`luau` feature only, no JIT; `Cargo.toml:79` [V✓]); 2 VMs per process; new userdata per Instance access, userdata `Vector3` (`script/src/types.rs:93, 926-960`) [V]; custom env per script may lose safeenv fast paths [I]; `luau0-src` pinned `=0.22.0` [V✓] | Blueprint VM; C++ native [K] | GDScript typed opcodes [K] | C# IL2CPP/Mono + Burst [K] | Luau interpreter + `--!native` codegen, native `vector` [F]/[K] |
| Script scheduler | `Vec<Waiting>` partitioned + sorted every step, new coroutine per handler per fire, connection prune every frame (`script/src/scheduler.rs:101-113`, `events.rs:211-245`) [V] | — | — | — | Deferred resumption points; thread reuse [F]/[K] |
| Script parallelism | None | Tasks/Pipes [F] | Threads + `call_deferred` [F] | Jobs [K] | Parallel Luau Actors, desync/sync, per-API safety levels [F] |
| Determinism | Ordered containers, fixed physics step, game-time clock (S §4) [V]; not cross-machine | Not a goal by default [K] | — | — | Not a goal [K] |
| Perf regression tracking | Benchmarks exist but `#[ignore]` (`player/src/bench_tests.rs:235, 288`) [V] | Insights + CSV profiler [K] | Profiler/monitors [K] | Profiler, Performance Testing package [K] | MicroProfiler [F] |

### 3.2 Narrative

**Parity.** The renderer is the most mature subsystem: clustered lights, instanced batching, frustum culling per shadow view,
LOD cross-fade, GPU skinning/particles, texture budget, deterministic rayon planning (R §5). That is roughly Godot Forward+
feature parity in the CPU-planned era. Fixed-step physics and the deterministic frame order are at parity with Godot/Unity.
The deferred event model matches Roblox [F].

**Behind.**
1. **No schedule or task graph.** The four peers all have a named stage list (tick groups, `PlayerLoop`, RunService events,
   `_process/_physics_process`) and a worker pool; Axcenity has prose in `docs/architecture/frame-loop.md` and four hand-copied
   loops that have already drifted (T §1.4). This blocks stage parallelism and makes every new feature a 4-host edit.
2. **Pull-every-frame instead of push-on-change** for draw list, physics sync, nav fingerprint, audio sync — ≈15–20 full-tree
   walks per frame [I] (S §3.1). Godot and Unity push dirty state to back-ends.
3. **No render graph** — every peer except Roblox (opaque) converged on one [F]. Today's single encoder costs only 1.2 ms
   (perf doc), so this is a scalability/maintainability gap rather than a current hotspot.
4. **Luau is used without its performance levers**: no native codegen, no `vector` type, per-access userdata and `String`
   allocation, new coroutine per handler. Roblox's own guidance [F] is selective `@native` on numeric code plus typed
   `Vector3` args; Axcenity cannot benefit until Vector3 is a native `vector`.
5. **rapier runs without `parallel`/`simd`** and with `wake_up_all`; Roblox's adaptive per-assembly stepping [K] and sleeping
   discipline are far more economical.

**Best fit to borrow.**
- **Roblox's RunService stage names as the schedule** (PreRender, PreAnimation, PreSimulation, PostSimulation, Heartbeat [F]):
  creators already know them, and they map 1:1 onto Axcenity's numbered steps. Implement as one `GameLoop` crate (T §6.1)
  whose stages are data, like Unity's `PlayerLoop` [K], so hosts insert hooks instead of copying order.
- **Unreal's Pipes idea via rayon** for parallelism inside stages: the `World` stays single-writer, but read-only stages
  (draw-list build per Workspace subtree, animator posing, nav build, audio sync) run as rayon tasks over `&World`, with
  writes queued and applied serially — the same "compute parallel, write serial" contract as Parallel Luau [F]. Rust's
  borrow checker enforces Unity's job-safety guarantees [K] for free.
- **Godot's servers + Unity's GPU Resident Drawer for render**: retained per-Instance draw records keyed by `revision`
  (push), a persistent GPU instance buffer for static items, then a shared mesh arena enabling `multi_draw_indexed_indirect`
  (wgpu supports it behind a feature) for GPU culling later. Nanite is out of scope for a wgpu engine.
- **A small RDG-style pass graph** over `ScenePipeline` (transient texture aliasing + pass culling [F]) when post-processing
  grows further; not urgent.
- **Luau**: enable mlua's codegen (`luau-jit`), map Vector3 to native `vector`, intern userdata; adopt Roblox's thread-reuse
  for non-yielding handlers. Parallel Luau Actors (one `Lua` state per actor) are a later, opt-in step.

---

## 4. Pillar 4 — Developer usability and extensibility

### 4.1 Comparison

| Dimension | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| Gameplay scripting | Luau (sandboxed allowlist env per script, watchdog 3 s budget, source-only loading) (`script/src/sandbox.rs:220-261`, `watchdog.rs`) [V]; luau-lsp definitions shipped (`script/src/definitions/`) [V] | C++, Blueprints, Verse (UEFN only) [K] | GDScript (gradual typing), C#, GDExtension [K]/[F] | C# (+Burst) [K] | Luau with gradual types, native codegen [K]/[F] |
| Visual scripting | Blocks ↔ Lua round-trip with a separate hand-written Luau reader (`blocks/src/lua.rs`, 1,403 LOC) [V]; node graphs, UI event graphs, behavior trees (`blocks-runtime`) [V] | Blueprints [K] | (VisualScript removed in 4) [K] | Visual Scripting package [K] | None first-party [K] |
| Editor | egui, single 162k-LOC crate, `EditorApp` 109 fields (`editor/src/app.rs:181`) [V] | Slate C++ editor, modular [K] | Editor built from engine's own Control nodes [K] | IMGUI/UI Toolkit editor [K] | Studio (Qt) [K] |
| Editor extension | Lua plugins: toolbar, buttons, dock widget = game ScreenGui, settings, project files, Selection, ChangeHistoryService; **no** property editors, importers, gizmos, menus, native code (`plugins/src/api.rs`, `lib.rs:67-81`) [V] | C++ plugins/modules, Editor Utility Widgets, Python [K] | `EditorPlugin` + `@tool` scripts; GDExtension native [K]/[F] | `Editor`/`EditorWindow`/`PropertyDrawer`, UI Toolkit [K] | Luau plugins with toolbar, DockWidgetPluginGui, ChangeHistoryService [K] |
| Native extension / packages | None; extension = editing workspace crates. No plugin ABI (T §4.7) [V] | `.uplugin` with modules + content, loading phases [K] | GDExtension shared libs (experimental, minor-version ABI) [F] | UPM packages, asmdef assemblies [K] | Packages (versioned assets), no native [K] |
| Code modularity | 20+ workspace crates, hakari; `script` god-crate depends on 11 engine crates, server links wgpu/cpal [I] (S §2, §3.6) [V] | Modules with Build.cs deps [K] | Monolithic C++ engine + modules [K] | Assembly definitions [K] | n/a |
| Build / iteration | Editor incremental 5–6 s, cold ~6 min, `-p player` 5 s with hakari (`docs/development.md`) [V]; engine-source watcher → background cargo build + relaunch with session handoff; subsecond hot-patch in debug (T §3.3) [V] | Live Coding (Live++), class-layout limits [K] | Script hot reload into running game; engine rebuild for C++ [K] | Domain reload; Enter Play Mode options [K] | Instant Play, no compile [K] |
| Play-in-editor | Cloned world, `catch_unwind` per frame, breakpoints/pause, multi-client Play over in-memory loopback (`editor/src/play.rs:1093`, T §5) [V]; step order differs from player (T §1.4) [V] | PIE with multiple clients [K] | Run project as separate process [K] | Play Mode in-process [K] | Play/Play Here/multi-client test servers [K] |
| Testing / CI | CI: fmt, hakari check, 3-OS `--locked` build, cargo-tree "player has no editor code" gate, clippy `-D warnings`, nextest, doctests (`.github/workflows/ci.yml`) [V]; gaps: unpinned toolchain (`rust-toolchain.toml` `stable` [V✓]), GPU tests skip without adapter, benches `#[ignore]`, server/test runner not shipped; `testing` crate with `axcenity-test` CLI + JUnit [V] | Automation framework, Gauntlet [K] | GUT/unit tests (community) [K] | Unity Test Framework [K] | TestService (limited), Open Cloud for CI [K] |
| Release | No `[profile.release]` tuning (`Cargo.toml` [V✓]); no signing/notarization (T §4.9) [V] | — | Export templates [K] | Build pipeline [K] | Publish to platform [K] |
| Dependency coupling | eframe 0.32 / egui_dock / wgpu 25 / winit 0.30 locked together because the viewport shares eframe's device (T §4.6) [V]; mlua ↔ `luau0-src` exact pin (`Cargo.toml:77-84` [V✓]) | — | — | — | — |
| Collaboration | Git + text files + version history under `.axcenity/history` (T §3.1) [V] | Source control + OFPA [F] | Git-friendly text [K] | Git + YAML; Unity VCS [K] | Team Create, Script Drafts [K] |

### 4.2 Narrative

**Ahead.** For a Roblox-like audience Axcenity offers something Roblox does not: plain project folders with diffable world
JSON and Lua files editable in VS Code with luau-lsp definitions, plus a blocks ↔ Lua round-trip. CI is stronger than typical
indie engines (cross-OS, dependency-layering gate). Multi-client Play over the real `net` code is at parity with Roblox Studio's
test servers [K]. The editor dogfoods the game UI for plugin dock widgets, mirroring Godot [K].

**Parity.** The plugin API surface (toolbar, dock widget, Selection, ChangeHistoryService) is a subset of Roblox Studio's [K],
which is the right model; it just stops early.

**Behind.**
1. **No native extension point.** Godot (GDExtension [F]), Unreal (`.uplugin` modules [K]) and Unity (UPM packages + asmdefs
   [K]) all let third parties add importers, node types and editor tools without forking. Axcenity requires editing a 162k-LOC
   crate.
2. **Editor and `script` are god-crates.** Unity's assembly definitions [K] exist precisely to cut recompile scope and enforce
   dependencies; Axcenity's 162k-LOC editor crate and 11-dependency `script` crate do the opposite (any `render` edit
   recompiles script, net, server, player, editor).
3. **Engine-version lock-in** (egui/wgpu/winit) has no peer analogue and blocks renderer upgrades.
4. **CI lacks perf gating and real GPU coverage**; none of the peers' public toolchains are a fair comparison here, but the
   project already has the benchmarks to gate on.

**Best fit to borrow.**
- **Roblox Studio's plugin model, extended** — `plugin` global, `ChangeHistoryService` waypoints for undo [K] — is already the
  base; extend with custom property editors, importers, menu items and viewport gizmos, as Godot's `EditorPlugin` does [K].
- **Godot's GDExtension concept, in Rust form**: a stable `abi_stable`/C-ABI plugin crate is premature; instead copy Unity's
  asmdef discipline internally first — split editor panels and `script` gameplay systems into crates behind traits
  (`EditorContext`, `GameSystem` registering Lua API + schedule stage). That gives the modularity benefits now and an
  extension seam later.
- **Godot's "script reload into running game"** [K] already exists in editor Play; extend it to the player's `--watch` path.

---

## 5. Scorecard (1 = absent/weak, 5 = industry-leading)

Scores for peers reflect the EF facts and their provenance; a [K]-only basis is noted as such.

| Pillar | Axcenity | Unreal 5 | Godot 4 | Unity 6 | Roblox |
|---|---|---|---|---|---|
| 1. Scene & ECS | **2.5** | 5 | 4 | 4.5 | 4 |
| 2. Assets & streaming | **2** | 5 | 3.5 | 4.5 | 4 |
| 3. Runtime perf & execution | **2.5** | 5 | 3.5 | 4.5 | 4 |
| 4. Usability & extensibility | **3** | 4 | 4.5 | 4.5 | 4 |

One-line justifications:
- **P1 Axcenity 2.5** — Right Roblox-shaped model with deferred events and safe generational IDs, but string-keyed BTree
  storage, no class/tag index and a global structure version make every system O(N) per frame.
- **P1 Unreal 5** — Reflected UObject/Actor model plus Mass archetype ECS with command buffers [F]; heavy but complete.
- **P1 Godot 4** — Node tree + ClassDB/Variant + servers/RIDs [F] is the cleanest dynamic-front/fast-back split; no bulk ECS.
- **P1 Unity 6** — Mature GameObject model plus DOTS chunks [F]; two models to learn, fragmentation pitfalls [F].
- **P1 Roblox** — Reference DataModel with deferred signals and Parallel Luau [F]; internals opaque [K].
- **P2 Axcenity 2** — Good text formats, migrations and Roblox-style instance streaming, but no import/cook, no stable IDs,
  no compressed textures, sync loads under a global lock.
- **P2 Unreal 5** — Cook + IoStore, soft refs, async loader, World Partition/OFPA/HLOD [F]/[K]; the reference.
- **P2 Godot 4** — Import sidecars + UIDs + threaded loader [K]/[S]; no built-in world streaming [K].
- **P2 Unity 6** — GUID AssetDatabase, Addressables, SubScene streaming [K]; complex but complete.
- **P2 Roblox** — Cloud asset IDs, moderated pipeline, StreamingEnabled [F]; not file-based.
- **P3 Axcenity 2.5** — Strong CPU-side renderer and deterministic order, but single-threaded sim, full per-frame rebuilds,
  no render graph or GPU-driven path, Luau without codegen/vector.
- **P3 Unreal 5** — Tasks/Pipes, multi-thread render, RDG, Nanite [F]/[K].
- **P3 Godot 4** — WorkerThreadPool, Forward+ clustered, RD graph; separate render thread still flagged buggy [F].
- **P3 Unity 6** — Jobs + Burst + GPU Resident Drawer + Render Graph [F]/[K].
- **P3 Roblox** — Luau native codegen, Parallel Luau, adaptive physics [F]/[K]; render internals opaque.
- **P4 Axcenity 3** — Git-friendly project, VS Code + luau-lsp, blocks, strong CI and multi-client Play; no native extension
  point, god-crates, version lock-in.
- **P4 Unreal 5** — Plugins/modules, Live Coding, Blueprints [K]; slow iteration on C++.
- **P4 Godot 4** — GDExtension + EditorPlugin + small fast editor [F]/[K]; ABI per minor version [F].
- **P4 Unity 6** — UPM, asmdef, rich editor scripting [K]; domain reload cost.
- **P4 Roblox** — Instant Play, Luau plugins, Team Create, Open Cloud [K]; closed platform, no native extension.

---

## 6. Biggest gaps, ranked by impact

Impact = (frame-time or shipping consequence) × (how many subsystems it unblocks), adjusted for Roblox-like goals.

1. **No class/tag index and allocating tree walks (P1/P3).** ~15–20 full Workspace walks/frame across scene, physics, audio,
   anim, nav, procedural, script (`core/src/world.rs:1194` [V✓]; S §3.1). Peers: Roblox `GetTagged`, Godot groups,
   DOTS queries. Fix: per-class and per-tag `InstanceId` sets in `World`, non-allocating iterator. Unblocks everything below.
2. **Full per-frame draw-list rebuild (P3).** `scene/src/draw_list.rs:449-529` [V✓]; 3.3 ms/10k parts. Borrow Godot
   servers: retained draw records keyed by `revision`, `ItemChunk`-style static groups (R §6.1).
3. **Synchronous asset loading under one global mutex; no preload in player (P2).** `assets/src/lib.rs:107, 236-264` [V✓].
   Borrow Godot `load_threaded_*` [S] + Roblox `PreloadAsync` [K]. Directly causes shipped-game hitches.
4. **No shared schedule; four drifting host loops (P3/P4).** T §1.4 [V]. Borrow Roblox RunService stage names [F] + Unity
   `PlayerLoop` data-driven stages [K]. Prerequisite for any parallelism and for correctness ("works in Play, breaks in player").
5. **String-keyed `BTreeMap<&str, Value>` property storage + per-access Lua `String`/userdata allocation (P1/P3).**
   `core/src/world.rs:55`, `script/src/instance.rs:114-121` [V✓]. Borrow Godot ClassDB/StringName: per-class slot indices;
   intern userdata (also fixes Instances-as-table-keys, S §3.4 [I, likely bug]).
6. **Physics sync coarseness: global `structure_version`, self-induced revisions, `wake_up_all` (P3).**
   `physics/src/lib.rs:782-905, 975` [V✓]. Add per-Workspace structure version / added-removed lists, physics-owned revision,
   localized wakes; consider rapier `parallel`/`simd` features.
7. **No asset pipeline: no stable IDs, no cook, RGBA8 + CPU mips, raw uncompressed `.axgame` (P2).** T §3.1, R §3.5 [V].
   Borrow Godot `.import` + `uid://` with BC7/ASTC and offline mips.
8. **Luau without performance levers (P3).** mlua `luau` only (`Cargo.toml:79` [V✓]), userdata Vector3, per-fire coroutines,
   O(W) scheduler. Borrow Roblox: native `vector`, `@native` codegen [F], thread reuse; binary-heap scheduler.
9. **No native extension / god-crates (`editor` 162k LOC, `script` → 11 crates; server links wgpu/cpal) (P4).** Borrow Unity
   asmdef discipline internally, then a Godot-EditorPlugin-like Lua surface for importers/property editors/gizmos.
10. **No GPU-driven path or render graph; egui/wgpu version lock (P3/P4).** No indirect draws, per-mesh buffers, full instance
    rewrite (R §3.4); viewport shares eframe's device (T §4.6). Borrow Unity GPU Resident Drawer shape (persistent instances +
    indirect) and a minimal RDG; decouple the viewport device when next upgrading wgpu.

Honourable mentions: JSON wire format for replication and unsecured netcode (S §3.7), synchronous navmesh rebuild every 0.5 s
when any prop moves (S §3.8), CI without perf gating or real GPU runs and an unpinned toolchain (T §4.9).
