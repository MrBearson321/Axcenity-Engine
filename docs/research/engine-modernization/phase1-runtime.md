# Phase 1A — Runtime core audit: `core`, `scene`, `render`, `assets`, `terrain`

Auditor: Phase 1 part A. Repo read-only at `/home/user/axcenity-engine`. Date 2026-11-10.
Legend: **[V]** = verified by reading the code (file:line cited). **[I]** = inference (reasoned from code/doc, not measured).
Paths are relative to `crates/` unless noted.

---

## 0. TL;DR

- The data model is a **generational arena of fat, dynamically-typed Instances**: each Instance owns a `BTreeMap<&'static str, Value>` of ~28 properties for a Part, looked up by string on every read. No SoA, no per-class structs, no component storage.
- **Change tracking exists but the render path ignores it**: `Instance::revision`, `World::structure_version` and a `ChangeLog` exist (used by net/physics/streaming), yet `scene::build_draw_list` **rebuilds every `DrawItem` from scratch every frame** by walking the whole Workspace and re-reading every property by name.
- `render` is the most optimised crate: instanced batches, frustum culling, LOD with hysteresis, GPU skinning, GPU particles, rayon-parallel planning, texture budget, cached materials/meshes by key. But it is **CPU-planned, not GPU-driven** (no indirect draws, one vertex/index buffer per mesh, full instance-buffer rewrite each frame, shadow casters' instance data copied per view).
- **Asset loading is synchronous by default and holds a global mutex**: `Assets::model` parses glTF/FBX, decodes embedded textures and runs meshopt LOD simplification **inside `Assets`' single `Mutex`**, on whichever thread first asks (usually the main thread during `build_draw_list`). The player never turns on background texture loading; GPU upload threading is only on while streaming is enabled.
- Everything runs on one thread in a fixed order; `World` is a plain owned value passed as `&`/`&mut` (no locks, no `Rc<RefCell>`). That is simple and correct, but means the draw-list build cannot overlap simulation.

---

## 1. Subsystem map

### LOC (from `wc -l`; `rs` includes tests; tests counted where in dedicated test files)

| Crate | `.rs` LOC | of which dedicated test files | `.wgsl` LOC | Largest files |
|---|---|---|---|---|
| `core` | 11,221 | 871 | – | `class.rs` 3,284; `world.rs` 1,252; `terrain.rs` 871; `input.rs` 791; `enums.rs` 773 |
| `scene` | 11,313 | 123 (+ inline tests) | – | `draw_list.rs` 2,105; `particles.rs` 1,213; `pick.rs` 971; `material.rs` 941; `terrain.rs` 897; `foliage.rs` 813 |
| `render` | 34,728 | 10,767 | 5,112 | `renderer/scene.rs` 2,797; `renderer/tests.rs` 2,378; `draw_list.rs` 1,751; `graphics.rs` 1,704; `renderer/lighting.rs` 1,369; `renderer/gpu.rs` 1,007; `renderer/temporal.rs` 1,001 |
| `assets` | 11,879 | 1,582 | – | `model.rs` 1,639; `material.rs` 1,467; `model/fbx.rs` 1,152; `lib.rs` 1,052 |
| `terrain` | 4,351 | 340 | – | `brush.rs` 1,051; `generate.rs` 609; `files.rs` 470; `erosion.rs` 416 |
| **Total** | **73,492** | | **5,112** | |

### `core` — Instance tree (deps: `thiserror` only)

| Module | Role | Key types |
|---|---|---|
| `arena.rs` (280) | Generational arena: `Vec<Slot<T>>` + free list; second slot space above 2³¹ for replica-local IDs | `Arena<T>`, `Id {index:u32, generation:u32}` |
| `world.rs` (1,252) | The tree, all mutation, change log, side tables | `World`, `Instance`, `InstanceId`, `Change`, `RecordedChange` |
| `class.rs` (3,284) | `ClassName` enum (Copy), class hierarchy predicates, **default property lists per class** built as `Vec<(&'static str, Value)>` | `ClassName` |
| `value.rs` (399) | Dynamic property value enum + math structs | `Value` (16 variants), `ValueKind`, `Vector3`, `CFrame`, `EnumItem{String,String}` |
| `enums.rs` | Material presets etc. | `Material`, `PartType`, ... |
| `terrain.rs` (871) | Heightfield/paint/foliage arrays with per-chunk version counters | `TerrainData` (kept in `World::terrains` as `Arc`) |
| `streaming.rs` (499) | Splits Workspace into streaming units, refreshed on `structure_version` | `Partition`, `Unit` |
| `prefab.rs`, `spline.rs`, `tool.rs`, `csg.rs`, `input.rs`, `animation.rs`, `dialogue.rs`, `ability.rs`, `ui_settings.rs`, `strings.rs`, `collision_groups.rs` | Domain helpers that walk the tree | |

Data flow: hosts own one `World` (player `crates/player/src/main.rs`, editor `App`). Systems take `&World`/`&mut World` in fixed frame order (`docs/architecture/frame-loop.md`). Side state that isn't a property lives in `World` hash maps keyed by `InstanceId` (`animators`, `property_animations`, `animation_tracks`, `terrains`; `core/src/world.rs:272-275`).

### `scene` — World → `DrawList` (deps: `core`, `assets`, `render`, `terrain`, `ui`; no third-party)

| Module | Role |
|---|---|
| `draw_list.rs` (2,105) | `build_draw_list*`: walks Workspace, emits `DrawItem`s/lights/probes/decals; MeshPart → per-primitive items (`ModelMesh` adapter implements `render::MeshSource`) |
| `material.rs`, `surfaces.rs` | Part material presets, material-variant files, procedural noise surfaces |
| `terrain.rs` (897) | `TerrainCache`: per-chunk meshes + weight textures, version-keyed; `ChunkWorker` thread when streaming |
| `foliage.rs` (813) | `PaintedFoliage`: terrain foliage pre-built into `ItemChunk`s (48-stud squares), cached by `FoliageKey` |
| `streaming.rs` (336) | Host-side streaming: hidden set of InstanceIds; ticks every 0.2 s |
| `particles.rs` (1,213) | Host-owned CPU particles; big emitters handed to GPU |
| `pick.rs` (971) | Editor/game picking, occluder gathering |
| `camera.rs`, `orientation.rs` | `FlyCamera`, YXZ degrees |
| `time_of_day.rs`, `fog.rs`, `clouds.rs`, `weather.rs`, `outlines.rs`, `color_grading.rs`, `lens.rs`, `graphics_volumes.rs`, `light_probes.rs`, `decals.rs`, `constraints.rs` | Lighting/post: read Lighting/Workspace into `DrawList` fields |
| `preload.rs` | `asset_files`: list models/textures for a loading screen (only the editor uses it: `editor/src/loading.rs:221`) |

### `render` — wgpu renderer (deps: wgpu, glyphon, glam, bytemuck, rayon, rustc-hash, serde, log, anyhow)

| Module | Role |
|---|---|
| `draw_list.rs` (1,751) | Public input contract: `DrawList`, `DrawItem`, `ItemChunk`, `Shape`, `Material`, traits `MeshSource`/`TextureSource` (data handed by `Arc<dyn …>`, keyed by `u64`) |
| `renderer.rs` (546) | `Renderer` (owns window surface; player) and `TextureRenderer` (draws into a texture on eframe's device; editor). Both wrap private `ScenePipeline` |
| `renderer/scene.rs` (2,797) | `ScenePipeline::draw` — the whole frame: lights, env/probes, LOD resolve, uploads, plan, shadows, prepass, scene, post, UI |
| `renderer/lighting.rs` (1,369) | `plan_lights` (clustered/tiled lights), `plan_draws` (sort + batch + instance data) |
| `renderer/lod.rs`, `culling.rs`, `parallel.rs` | LOD hysteresis/fades, frustum `Reach`, rayon helpers (512-item shares; threshold 4,096 items) |
| `renderer/gpu.rs`, `uploads.rs`, `texture_budget.rs` | Mesh/texture upload, CPU mip generation, optional upload thread, memory budget |
| `renderer/skinning.rs`, `gpu_particles.rs`, `sprites.rs`, `decals.rs`, `terrain.rs`, `water.rs`, `temporal.rs`, `post.rs`, `occlusion.rs`, `fog.rs`, `environment.rs`, `probes.rs`, `probe_bake.rs`, `light_probes.rs`, `realtime_gi.rs`, `text.rs`, `ui_pass.rs`, `turned_text.rs`, `surface_shaders.rs`, `debug_views.rs`, `gpu_timer.rs` | Feature passes |
| `graphics.rs` (1,704) | `GraphicsSettings`, presets, `Capabilities` |
| 19 `.wgsl` files (5,112) | `shader.wgsl` 877, `lighting.wgsl` 986, ... |

### `assets` — files → plain data (deps: gltf, ufbx, image, meshopt, glam, serde, serde_json, thiserror)

| Module | Role |
|---|---|
| `lib.rs` (1,052) | `Assets` = `Arc<Mutex<Shared>>` cache of models/textures/sky images/probes/materials/data tables/fonts keyed by path `String`; optional 4 texture worker threads |
| `model.rs` (1,639), `model/fbx.rs`, `model/rig.rs`, `model/lod.rs`, `model/maps.rs`, `model/objects.rs` | glTF/FBX → `Model { primitives: Vec<Primitive> }`, skins, takes, meshopt LODs, texture-role detection |
| `material.rs` (1,467) | `*.material.json` with base chains |
| `reload.rs` (510) | Live reload by (mtime,size) stamp; swaps in a new key |
| `import.rs`, `texture_set.rs`, `sky_image.rs`, `probe.rs`, `light_probes.rs`, `color_grading.rs`, `data_table.rs`, `fonts.rs` | Misc. file kinds |

### `terrain` — algorithms over `core::TerrainData` (deps: core, glam, png, serde, serde_json, thiserror)

`brush.rs` (sculpt/paint/foliage scatter), `generate.rs`, `erosion.rs`, `noise.rs`, `materials.rs` (rules), `mesh.rs` (chunk meshes with distance LOD, parry-compatible triangle split), `raycast.rs`, `files.rs` (codecs), `job.rs` (cancellable thread jobs for generate/erode), `convert.rs`.

### Per-frame data flow (player)

```
World (owned by App) ──&World──► scene::Streaming::step   (player/src/main.rs:839, via step_streaming @1666)
                      └─&World──► scene::build_draw_list_with_ui_interaction (player/src/main.rs:1669)
                                    ├─ lighting_settings + graphics_volumes::blend  (walks Workspace)
                                    ├─ for every Workspace descendant: DrawItem(s)  (scene/src/draw_list.rs:455-529)
                                    │     └─ assets.model()/material()/texture()  [global Mutex]
                                    ├─ TerrainCache::draw_all (walks Workspace again; scene/src/terrain.rs:286)
                                    └─ ui::UiInteraction::layout → list.ui = layout.draw_items.clone()  (draw_list.rs:623)
DrawList ──&──► Renderer::render (player/src/main.rs:1808 → render/src/renderer.rs:334)
                 └─ ScenePipeline::draw (render/src/renderer/scene.rs:532)
                      LodStates::resolve → FrameUses → upload_new → plan_draws → shadows/prepass/scene/post/UI → present
```
Editor: `editor/src/viewport.rs:1289` (`build_draw_list_posed`) and `:1362` (`TextureRenderer::render`).

---

## 2. Dependency & provider matrix

Versions from workspace `Cargo.toml` `[workspace.dependencies]`. All crates also depend on `axcenity-workspace-hack` (hakari feature unification).

| Crate | Third-party dep | Version | What it powers | Hook into the main loop |
|---|---|---|---|---|
| core | `thiserror` | 2 | Error enums (`TreeError`, `PropertyError`, `AttributeError`; `core/src/world.rs:155-200`) | Not on the hot path |
| scene | *(none; dev: `image` 0.25)* | | | `build_draw_list*` called each frame (player `main.rs:1669`, editor `viewport.rs:1289`) |
| render | `wgpu` | 25 | All GPU work. Adapter `HighPerformance` (`render/src/renderer.rs:236`), device (`:246`), surface `get_default_config` (`:255`, no explicit present mode → wgpu default, i.e. Fifo/vsync [I]) | `Renderer::render` each frame (`renderer.rs:334`) → `ScenePipeline::draw` (`renderer/scene.rs:532`); editor shares eframe's device via `TextureRenderer::new` (`renderer.rs:370`, `editor/src/viewport.rs:1226`) |
| render | `glyphon` | 0.9 | UI text shaping/atlas (cosmic-text under it); one `TextRenderer` per UI layer | `prepare_ui_text` inside `draw` (`renderer/scene.rs:918-919`); fonts loaded lazily from `DrawList::fonts` (`scene.rs:909-915`) |
| render | `glam` | 0.34 | Math (`Mat4`, `Vec3`) in the draw-list contract and planning | Throughout |
| render | `bytemuck` | 1 | POD casts for uniform/instance uploads | `queue.write_buffer(... bytes_of ...)` (`scene.rs:709-781`), `write_pieces` (`renderer/parallel.rs:73`) |
| render | `rayon` | 1.10 | Parallel LOD resolve, material keys, sort keys, sort, instance data, shadow casters (shares of 512; only above 4,096 items or 1 MiB) | `renderer/parallel.rs:12-38`, used from `lod.rs:160`, `lighting.rs:790` |
| render | `rustc-hash` | 2 | `FxHashMap` for LOD states and per-frame material indexing | `renderer/lod.rs:139`, `renderer/lighting.rs:689,714`. Long-lived caches still use SipHash `std::HashMap` (`renderer/types.rs:926-931`) |
| render | `serde` | 1 | (De)serialize `GraphicsSettings` | Settings load, not per frame |
| render | `log`, `anyhow` | 0.4 / 1 | Diagnostics/errors | – |
| render (dev) | `pollster`, `serde_json` | 0.4 / 1 | Tests/benches | – |
| assets | `gltf` | 1.4 (+`KHR_texture_transform`, `extras`) | glTF/GLB parsing; `import_buffers` (`assets/src/model.rs:713`) | Lazily on first `Assets::model()` call — which is normally **inside `build_draw_list`** (`scene/src/draw_list.rs:1070`) on the main thread |
| assets | `ufbx` | 0.11 | FBX (C library via `cc`), memory-only load | Same lazy path |
| assets | `image` | 0.25 (bmp, exr, hdr, jpeg, png, tga) | Texture decode → RGBA8 (`model.rs:1453`, `lib.rs:863 read_texture`) | Main thread on first `Assets::texture()` unless `load_textures_in_background` was called (only editor: `editor/src/app.rs:1183`); then 4 worker threads (`assets/src/lib.rs:97,583`) |
| assets | `meshopt` | 0.7 | Mesh simplification for LODs at load (`assets/src/model/lod.rs:99-131`) | Inside model load, inside the `Assets` mutex |
| assets | `glam`, `serde`, `serde_json`, `thiserror` | | Rig math, material/data-table JSON, errors | Load time |
| terrain | `glam` | 0.34 | Brush/mesh math | Mesh build: `scene/src/terrain.rs` (main thread, or `ChunkWorker` when streaming) |
| terrain | `png` | 0.18 | Heightmap import/export codecs (`terrain/src/files.rs`) | Project load/save only |
| terrain | `serde`, `serde_json`, `thiserror` | | Generator settings, files | Load time |

Indirect/notable: `rayon` is the only parallelism framework in these crates; no async runtime anywhere; threads are hand-rolled `std::thread::spawn` (texture workers `assets/src/lib.rs:583`, upload thread `render/src/renderer/uploads.rs`, chunk worker `scene/src/terrain.rs:125`, terrain jobs `terrain/src/job.rs`).

---

## 3. Bottlenecks and friction (evidence-based)

### 3.1 Property storage: string-keyed BTreeMap of a dynamic enum per Instance

- **[V]** `Instance.properties: BTreeMap<&'static str, Value>` (`core/src/world.rs:55`), plus `attributes: BTreeMap<String, Value>`, `tags: Vec<String>`, `name: String`, `children: Vec<InstanceId>` (`world.rs:50-65`).
- **[V]** Every read is `properties.get(name)` with a `&str` (`world.rs:114-116`) — an O(log n) walk doing **string comparisons** at each node. `set_property` does `get_key_value` then `get_mut` (two lookups) plus a `kind()` check (`world.rs:1007-1024`).
- **[V]** `Value` has 16 variants including `String` and `EnumItem { enum_name: String, item: String }` (`core/src/value.rs:175-200`). **[I]** So `size_of::<Value>()` ≈ 56 bytes; a property entry ≈ 72 B before BTree node overhead.
- **[V]** A `Part` has 18 own properties + 4 collision + 5 body + `Locked` = **28 properties** (`core/src/class.rs:2015-2058`, 966-989). **[I]** ≈ 2–2.5 KB heap per Part spread over several BTree nodes, plus a heap `String` name (`world.rs:71`).
- **[V]** `Instance::new` builds a fresh `Vec<(&str, Value)>` from `default_properties()` and collects it into a BTreeMap (`world.rs:74`, `class.rs:966`) — several allocations per `Instance.new`, more for classes with `String`/`EnumItem` defaults.
- **[V]** `part_draw_item` reads ~12–15 properties by name per part per frame (`scene/src/draw_list.rs:783-876`: `Color`, `Material`, `TileSize`, `Transparency`, `Emission`, `EmissiveColor`, `CastShadow`, `MaterialVariant`, `BlueprintTexture`, `Shape`, plus `part_placement` reading `Position`/`Orientation`/`Size`).
- Impact **[I]**: per-instance CPU cost is dominated by pointer-chasing and string compares rather than math; cache locality is poor (each Instance's properties live in separate BTree nodes). This is the root of the `build_draw_list` numbers in `docs/rendering/performance.md` (3.3 ms for 10,000 parts; 6.1 ms for 50,000 streamed parts in `docs/architecture/streaming.md`).

### 3.2 Draw list is a full rebuild every frame; change tracking is not used by `scene`

- **[V]** `build_draw_list_posed` allocates new `items`, `lights`, ... and loops over `world.descendants(world.workspace())` (`scene/src/draw_list.rs:449-529`). There is no cache keyed by `Instance::revision`; every Part's `DrawItem` (including its `Material`) is reconstructed.
- **[V]** `World` already has what an incremental path needs: `Instance::revision` bumps on any name/parent/property/attribute/tag change (`world.rs:62-87`, 1032, 1047), `structure_version` on reparent (`world.rs:815`), and an opt-in `ChangeLog` (`world.rs:206-253, 564-571`). Net replication and physics use revisions (`docs/architecture/streaming.md`, "What it costs"); `scene` does not.
- **[V]** Only terrain and painted foliage are cached across frames: `TerrainCache` keeps chunk meshes by per-chunk version (`scene/src/terrain.rs:53-69, 383-388`); `PaintedFoliage` keeps `ItemChunk`s until `FoliageKey` changes (`scene/src/foliage.rs:405-420`).
- **[V]** `descendants()` allocates a `Vec<InstanceId>` of the whole subtree every call (`core/src/world.rs:1194-1208`). Per frame the Workspace is walked at least: main draw loop (`draw_list.rs:455`), `TerrainCache::draw_all` (`scene/src/terrain.rs:286`), `graphics_volumes::blend` (`scene/src/graphics_volumes.rs:45`, called from lighting settings), and particles (`scene/src/particles.rs:641`) — **≥4 full Workspace walks + Vec allocations per frame** [V for the call sites; per-frame frequency of `blend` inferred from `lighting_settings` at `draw_list.rs:441`].
- **[V]** Per instance, filters walk ancestors: `csg::out_of_world` → `in_union` walks to the root, and `tool::is_stored` → `carried` walks again (`core/src/csg.rs:3-19`, `core/src/tool.rs:74`). `graphics_volumes::blend` runs `out_of_world` on every descendant too. `Characters::contains` walks ancestors but memoises (`scene/src/decals.rs:182-207`). So **O(N × depth)** per frame.
- **[V]** `Streaming::is_hidden` is a SipHash `HashSet<InstanceId>` lookup per instance (`scene/src/streaming.rs:46-48`); the set is rebuilt from scratch every 0.2 s tick (`streaming.rs:93-112`).
- **[V]** Fat `DrawItem`: `Material` holds `surface_parameters: [[f32;4]; 8]` (128 B), up to ~9 `Option<Arc<dyn TextureSource>>`, `Option<Water>`, `Shading`, etc. (`render/src/draw_list.rs:1222-1254, 1446-1470, 1576, 1607+`). **[I]** a few hundred bytes per item; built, moved, and dropped each frame. Capsules `..item.clone()` per piece (`draw_list.rs:504-508`).
- **[V]** UI draw items are cloned every frame: `list.ui = layout.draw_items.clone()` (`scene/src/draw_list.rs:623`). `DrawList::fonts` is re-collected each frame (cheap: `Arc<Vec<u8>>` + path `String` clone; `draw_list.rs:550-559`, `assets/src/fonts.rs:9-12`) and the renderer re-hashes each font path each frame (`render/src/renderer/scene.rs:909-910`).

### 3.3 MeshParts: per-frame allocation and locking

- **[V]** For each MeshPart each frame: `assets.model(mesh_id)` locks the global `Assets` mutex and does a SipHash `HashMap<String,…>` lookup (`assets/src/lib.rs:232-238`); `material::variant_of` → `assets.material(path)` locks again (`scene/src/material.rs:20-28`); a `Vec<&Instance>` of SurfaceAppearances is collected (`draw_list.rs:1084-1089`).
- **[V]** For each primitive: `Arc::new(ModelMesh::new(&model, index))` (`draw_list.rs:1120`), whose constructor allocates a `Vec<OnceLock<…>>` for LOD copies (`draw_list.rs:1372-1380`). Because a new `ModelMesh` is made every frame, its `OnceLock` LOD cache is discarded every frame, so `lod()` allocates a new `Arc<ModelMesh>` for each LOD level actually used, once per frame (`draw_list.rs:1420-1430`). **[I]** The doc's claim that `MeshSource::lod` "hands out a kept copy" (`docs/rendering/performance.md`, threading section) holds within a frame only.
- **[V]** Posed MeshParts copy all joint matrices into a fresh `Arc<Skin>` each frame: `Arc::new(Skin::new(posed.joints().to_vec()))` (`draw_list.rs:1102-1104`).
- **[V]** `draw_id(id)` uses `DefaultHasher` (SipHash) per item per frame (`draw_list.rs:1276-1280`).

### 3.4 Renderer CPU planning (already heavily optimised, but per-frame O(items))

- **[V]** `ScenePipeline::draw` per frame: `LodStates::resolve` over all items + kept chunks (`renderer/scene.rs:832-836`, `renderer/lod.rs:152-226`) → `FrameUses::of` → `upload_new` → `plan_draws` (sort, batch, build `InstanceRaw`s) → `casters_in_views` → build one big instance upload (`scene.rs:837-908`).
- **[V]** The instance buffer is **rewritten in full every frame**, and each shadow view appends its own copy of its casters' instances after the scene's (`scene.rs:886-908`). The doc notes `InstanceRaw` is 176 B of which the shadow pass reads 88 (`docs/rendering/performance.md`, "What's left").
- **[V]** `upload_new` still iterates **every item in `list.items`** calling `note_texture` for each texture (hash lookup in SipHash `HashMap<u64,…>`) even though materials are deduplicated just below (`renderer/scene.rs:2078-2101` vs `2104-2114`).
- **[V]** Long-lived caches use std SipHash maps: `custom_meshes`, `textures`, `full_textures`, `materials: HashMap<MaterialKey,…>` with a ~330-byte key (`renderer/types.rs:474-480, 926-931`). Per-frame planning uses `FxHashMap` (`renderer/lighting.rs:689,714`). Mixed hashing policy.
- **[V]** LOD state is `FxHashMap<(u64,u64), Mutex<LodState>>` — a mutex per item state so rayon workers can update in place (`renderer/lod.rs:130-140, 285+`). **[I]** fine at current scale; uncontended locks still cost an atomic pair per item per frame.
- **[V]** No GPU-driven path: no `draw_indexed_indirect`/`multi_draw` anywhere in `render/src` (grep). Each custom mesh owns separate vertex/index buffers (`renderer/gpu.rs:604-700`, `upload_buffers` `:687`), so batches can't be merged across meshes; terrain's 1,024 chunks are 1,024 meshes (`docs/rendering/performance.md` baseline: 1,027 batches → 5,135 draw calls before culling). Draw recording is single-threaded (one encoder, 16 `set_bind_group` sites in `scene.rs`) — measured at only 1.2 ms so not currently the bottleneck (perf doc).
- **[V]** Occlusion culling deliberately not built; reasoning recorded in `docs/rendering/performance.md` (Phase 15).

### 3.5 Asset loading: synchronous, under one global mutex

- **[V]** `Assets { shared: Arc<Mutex<Shared>> }` (`assets/src/lib.rs:107-109`); every accessor takes `self.lock()` (`lib.rs:758`).
- **[V]** `Assets::model` holds the lock while calling `read_model` → `std::fs::read` → `load_models` (glTF/FBX parse, embedded texture decode `model.rs:1453`, meshopt simplification `model/lod.rs:122`) (`assets/src/lib.rs:236-264`, `766-781`). A large model therefore blocks **every other `Assets` caller**, including the 4 background texture workers (which also need the lock to store results, `lib.rs:604-620`) and any other system calling `assets.model()` (physics hulls, animation).
- **[V]** Texture decode is synchronous on the caller's thread unless `load_textures_in_background` was called (`lib.rs:275-287`). Only the editor calls it (`editor/src/app.rs:1183`); **the player decodes textures and models synchronously on the main thread, lazily, during the first frame(s) that draw them** — there is no preload in the player (`scene::asset_files` is used only by `editor/src/loading.rs:221`). **[I]** => first-sight hitches for large textures/models in shipped games.
- **[V]** GPU uploads are synchronous inside `ScenePipeline::draw` (`note_texture` → `upload_texture`, `renderer/scene.rs:2251-2280`; `upload_custom_mesh`, `:2055-2070`) unless `set_background_uploads(true)`, which hosts enable **only when streaming is on** (`player/src/main.rs:839-843`, `editor/src/viewport.rs:1280`).
- **[V]** Mipmaps are generated on the CPU by a scalar 2×2 box filter (`renderer/gpu.rs:967-1000`) — this is why `render` is forced to `opt-level = 3` in dev (`Cargo.toml` profile comment: 1.2 s for a 4K map unoptimised).
- **[V]** Textures are uploaded uncompressed RGBA8 (no BCn/ASTC; grep finds no compressed formats). Decoded `Image { rgba: Vec<u8> }` stays in the `Assets` cache after upload (`assets/src/model.rs:463-469`, `lib.rs:139-141`). **[I]** CPU RAM ≈ GPU RAM for textures (e.g. 540 MB of embedded 2K/4K textures in the `bench_project` notes would also sit in CPU memory).
- **[V]** Cache keys are path `String`s with SipHash (`lib.rs:139-147`); failures are cached as `None` and not retried (`lib.rs:137-139`) — good for avoiding per-frame retries.

### 3.6 Terrain

- **[V]** Per-chunk height/paint versions let `TerrainCache` rebuild only edited chunks (`scene/src/terrain.rs:383-388`) — good.
- **[V]** Without streaming, chunk meshes are built on the main thread inside `build_draw_list` (`terrain.rs:393` branches on `background`, which is `streaming.settings().is_some()`, `:284`).
- **[V]** `World::terrain_mut` uses `Arc::make_mut` (`core/src/world.rs:449-451`). Background `ChunkJob`s hold `data.clone()` of the `Arc<TerrainData>` (`scene/src/terrain.rs:404-414`). **[I]** A brush stroke while a chunk job is pending deep-copies the entire heightfield, paint weights and foliage (`TerrainData::clone`, `core/src/terrain.rs:323-331`; ~1025² samples ⇒ 4 MB heights alone, plus paint).
- **[V]** `terrain_hit` walks the whole Workspace per ray (`scene/src/terrain.rs:724`); picking walks it per ray too (`scene/src/pick.rs:153, 261`). Perf doc shows earlier selection-box slowness from this pattern was mitigated by `Occluders`.

### 3.7 Arena and World

- **[V]** `Arena` has no iterator API (`core/src/arena.rs` public fns: insert/insert_at/remove/get/get_mut/contains/len). All whole-world iteration is via tree walks (`descendants`) — so there is no cheap "all Parts" query; every system that needs "all X" walks and filters the tree.
- **[V]** `Arena::insert_at` does `self.free.retain(...)`, O(free-list) per call (`arena.rs:133`); it's used for replica creation (`World::create_at`, `world.rs:727`). **[I]** quadratic-ish only if many slots are freed on the replica; minor.
- **[V]** `World` is `#[derive(Clone)]` (`world.rs:255-256`) and deep-clones every Instance's BTreeMaps/Strings. The editor clones the whole world for undo snapshots in ~20 places (`editor/src/app/edits.rs:950,975`, `commands.rs:183`, `solid.rs:187`, ...) and for Play (`engine.rs:491`, `session.rs:504`). **[I]** O(world size) memory and time per such edit; scope of the editor auditor, but rooted in core's data layout (no structural sharing; only `TerrainData` and `StringTable` are `Arc`).
- **[V]** Side tables (`animators`, `terrains`, …) are SipHash `HashMap<InstanceId,…>` (`world.rs:272-275`); `destroy` removes from each per descendant (`world.rs:947-966`).
- **[V]** `tagged()` and `all_tags()` walk the whole tree (`world.rs:1142-1160`): CollectionService-style queries are O(N) with no tag index.
- **[V]** `ChangeLog` is only active when `record_changes()` is on, and is a plain `Vec<RecordedChange>` with `String` payloads for attributes/tags (`world.rs:206-253`).

### 3.8 Threading / sharing of `World`

- **[V]** `World` is owned by the host app and passed by reference: e.g. `build_draw_list_with_ui_interaction(&self.world, &self.assets, …)` (`player/src/main.rs:1669-1687`); scripts/physics take `&mut World` (`player/src/main.rs:432, 452, 495`). No `Arc<Mutex<World>>`, no `RefCell`, consistent with the `CLAUDE.md` invariant.
- **[I]** Consequence: the frame is strictly serial — simulation (scripts, physics, animation) and `build_draw_list` cannot overlap, and `build_draw_list` itself is single-threaded (it is a sequential loop over `descendants`; rayon is only used in `render`). The perf doc's forest example had 40.8 ms on 1 of 32 threads before render planning was parallelised; `build_draw_list` remains serial.
- **[V]** `Assets` is shared across threads by `Arc<Mutex>` (see 3.5). `render::MeshSource`/`TextureSource` are `Send + Sync` and handed over as `Arc<dyn …>` so the renderer can read model data without copying (`render/src/draw_list.rs:1362, 1417`) — this is what enables the upload thread and rayon planning.

---

## 4. Ranked hotspots (CPU per frame, scaling with instance count)

| # | Hotspot | Cost driver | Evidence |
|---|---|---|---|
| 1 | Full `build_draw_list` rebuild | O(N) string-keyed property reads + ancestor walks + DrawItem/Material construction | `scene/src/draw_list.rs:449-529, 783-876`; 3.3 ms/10k parts, 6.1 ms/50k streamed parts (docs) |
| 2 | Renderer planning + full instance rewrite + per-view caster copies | O(drawn items × views) | `renderer/scene.rs:832-908`; 2.4–3.4 ms/10k parts; 9.5–10 ms forest at Ultra (docs) |
| 3 | Sync asset decode under a global lock (player) | Model/texture size; blocks other threads | `assets/src/lib.rs:232-264, 275-287` |
| 4 | Sync GPU uploads + CPU mips (non-streaming) | Texture size | `renderer/scene.rs:2251-2280`, `renderer/gpu.rs:967-1000` |
| 5 | Repeated Workspace walks + `Vec` allocs | ≥4 walks/frame | `world.rs:1194`; call sites in 3.2 |
| 6 | MeshPart per-frame `Arc`/`Vec` allocation, mutex locks | per MeshPart/primitive | `draw_list.rs:1070-1104, 1120, 1372-1380` |
| 7 | Terrain COW clone during streaming + edits | heightfield size | `core/src/world.rs:449`, `scene/src/terrain.rs:404-414` |

---

## 5. Strengths worth preserving

- **[V]** Clean layering: `render` knows nothing of Instances; it consumes plain `DrawList` data with `Arc<dyn MeshSource/TextureSource>` keyed by `u64` (`render/src/draw_list.rs:1334-1421`). This makes an incremental/retained scene layer or a different front-end possible without touching the renderer.
- **[V]** Generational IDs with stale-safe `Option` lookups, network-stable bits and a separate local ID space for replicas (`core/src/arena.rs`, `world.rs:25-47`).
- **[V]** Change-detection primitives already exist (`Instance::revision`, `structure_version`, `ChangeLog`, per-chunk terrain versions) and are used successfully by physics (`sync_changed_parts`), net replication and terrain — the foundation for an incremental draw list.
- **[V]** Renderer performance work is measured and documented with reproducible benchmarks (`docs/rendering/performance.md`, `player/src/bench_tests.rs`): instanced batching by mesh+material, frustum culling for camera and every shadow view, LOD with hysteresis/cross-fade, distance bands, GPU skinning (13× CPU win), GPU particles, alternating far cascades, depth prefill for alpha-tested foliage, texture memory budget, deterministic rayon planning (`planning_comes_out_the_same_however_many_threads_share_it`).
- **[V]** `ItemChunk` (pre-built `Arc<[DrawItem]>` groups with bounds + cull distance) is already a retained-mode path for foliage that `LodStates::resolve` can skip whole (`render/src/draw_list.rs:1164-1196`, `renderer/lod.rs:173-205`) — the obvious template for retained static geometry.
- **[V]** Loading never panics on bad files (`catch_unwind` around gltf/ufbx/decoders, `assets/src/model.rs:550-610`, `lib.rs:604-616`); failures are cached and reported once.
- **[V]** Live reload by (mtime,size) stamps with last-good fallback, new key forces GPU re-upload (`assets/src/reload.rs`).
- **[V]** Single-threaded, explicitly ordered frame (`docs/architecture/frame-loop.md`) shared by player, editor Play, server and test runner — deterministic and easy to reason about.
- **[V]** Dev-profile tuning keeps hot crates optimised in debug (`Cargo.toml` `[profile.dev.package.*]`).

---

## 6. Opportunities (for Phase 2 to evaluate; not prescriptions)

1. **Retained/incremental draw list**: cache each Instance's `DrawItem`s keyed by `(InstanceId, revision)` (plus ancestor-dependent flags invalidated by `structure_version`), rebuild only changed ones; move static parts into `ItemChunk`-like spatial groups. Highest leverage for hotspots 1, 5, 6.
2. **Typed property storage**: intern property names to small integer IDs per class (or per-class slot arrays/`SmallVec`), keep `Value` but index by slot; or SoA hot-path components (transform, color, material) beside the generic map. Reduces 3.1 costs without changing the scripting surface.
3. **Assets off the lock**: do I/O + decode + meshopt outside the mutex (insert a "loading" marker, finish on a worker), enable background texture/model loading in the player, and use the existing `asset_files` preload in the player's loading screen.
4. **GPU-side**: compressed textures (BC7/ASTC) with offline mip generation, a shared mesh arena (one vertex/index buffer) enabling multi-draw-indirect and GPU culling later; slimmer shadow instance stream (88 B) and persistent instance buffers for static items.
5. **Indexes in `World`**: class→ids and tag→ids indices maintained on create/destroy/reparent, plus a non-allocating descendant iterator; ancestor flags (`in_union`, `stored`) cached per `structure_version`.
6. **Hashing consistency**: `FxHashMap` (or `ahash`) for engine-keyed long-lived maps (`renderer/types.rs:926-931`, `World` side tables, `Streaming::hidden`); keep SipHash only for externally-controlled keys.

---

## 7. Verification notes / limits

- Not run: no builds, tests or benchmarks were executed; all timings quoted are from `docs/rendering/performance.md` and `docs/architecture/streaming.md` (AMD RX 9070 XT, Windows).
- `size_of::<Value>()` and per-Part memory are estimates from type definitions.
- `renderer/lighting.rs::plan_draws` internals and WGSL were skimmed, not read line by line; claims about them are limited to signatures and the documented behaviour cross-checked with call sites.
