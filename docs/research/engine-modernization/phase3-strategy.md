# Phase 3: Package and refactoring strategy (Axcenity Engine)

> **Superseded in part (2026-11-10).** These are working notes; the report (`../engine-modernization-report.md`) is the maintained version. Paul's answers to the open questions changed several verdicts here: GitHub Actions is intentionally off (M10 and every "CI gate" or "nightly" step are deferred or become local checks), cross-machine determinism is not a requirement, mobile and web are deferred (ASTC and wasm don't matter for now), Luau's native `vector` is not chosen for Vector3 until its type identity and interop are verified, and internet-hosted dedicated servers (M11) are deferred. See `../../development-policy.md`.

Date 2026-11-10. Inputs: `phase1-runtime.md` (R), `phase1-sim.md` (S), `phase1-tools.md` (T), `phase2-engine-facts.md` (EF),
`phase2-benchmark.md` (B), root `Cargo.toml`, `Cargo.lock`. Repo paths are relative to `/home/user/axcenity-engine`, crate paths to `crates/`.

**Markers.** [V] read in code (by Phase 1, or re-checked today, then marked [V✓]). [I] inference, not measured.
Crate facts marked **[cr]** were read today from the crates.io API (version, licence, MSRV, features, dependency ranges).
**[verify]** = not confirmed; check before acting. Engine-fact markers [F]/[S]/[K] are carried over from EF.

**Ground rules from `CLAUDE.md`.** The player never depends on editor, egui or blocks. `render` consumes only draw data.
Instances stay in the generational arena, with no `Rc<RefCell>`. There is one Lua runtime. Every dependency is pinned in the
workspace with an Apache-2.0-compatible licence. Each recommendation below was checked against these rules.

**The main caveat.** No audit ran a timing. All numbers come from `docs/rendering/performance.md` and
`docs/architecture/streaming.md` (3.3 ms build_draw_list for 10k parts; 6.1 ms for 50k streamed), or are [I].
For that reason, roadmap item **I1 (measure first)** gates every perf item.

---

## 0. New facts found in this phase

- [V✓] **`profiling` 1.0.18 is already in the lock**, pulled in by wgpu 25, egui 0.32 and rapier3d 0.36.1. It has
  `profile-with-tracy`/`-puffin`/`-tracing` features [cr]. If one workspace feature turns on its Tracy backend, the existing
  scopes in wgpu, egui and rapier [verify that rapier emits scopes] show up next to ours, with no new instrumentation API.
- [V✓] Existing benchmarks are **render-only**: `player/src/bench_tests.rs:235-460` (`bench_10000_parts`,
  `bench_2000_mesh_parts`, terrain+foliage, 20 animated characters), all `#[ignore]`. **No benchmark covers scripts, physics
  with moving bodies, the ChangeLog, replication or nav.** None of them run in CI.
- [cr] **mlua 0.12.2 is still the latest release.** mlua-sys 0.13.0 declares `luau0-src = "^0.22.0"`, but 0.22.1 already writes
  bytecode v13, which mlua-sys cannot read (`Cargo.toml:77-84`). That is a **semver bug upstream**: the exact pin is the
  correct defence and cannot be lifted until a new mlua-sys ships. luau0-src 0.23.0+luau745 exists but is outside the range anyway.
- [cr] **bincode is flagged unmaintained** (RUSTSEC-2025-0141; 3.0.0 is a terminal release). Do not adopt it.
- [cr] **Graphics stack target:** eframe 0.37.0 needs wgpu 30, but **egui_dock 0.21.1 and egui-phosphor 0.14.0 still target egui
  0.36**. The newest consistent set is eframe/egui **0.36.2** + egui_dock 0.21.1 + egui-phosphor 0.14.0 + wgpu **30** +
  glyphon **0.12.0** (wgpu 30) + winit 0.30.13 (unchanged). eframe 0.36 and egui_dock 0.21 need **MSRV 1.95**.
- [cr] **rapier3d 0.36.1 features:** `parallel` (rayon + parry parallel), `simd8`, `enhanced-determinism`, `profiler`. The
  feature called `simd-stable` in older releases no longer exists. The defaults are `dim3`, `f32` and `std`, which is what the engine uses today.
- [V✓] **The Instance-as-table-key bug is real in code shape.** `Value::Instance` → `LuaInstance(id).into_lua(lua)` creates a new
  userdata (`script/src/types.rs:954-958`). `__eq` is defined (`instance.rs:122-124`), but Lua table lookup uses raw
  identity, not `__eq`. So `t[part]` misses. There is no test yet.
- [V✓] Stale docs: `docs/development.md:39` says "CI is off"; `docs/architecture/frame-loop.md:13` says "editor Play has no
  network step"; `docs/architecture/crates.md:30` leaves `net` out of the editor's dependencies.
- [V✓] There are about 400 distinct property-name literals in `core/src/class.rs` across about 130 classes, and about 500 non-test
  `.property("…")` call sites. A static `PropertyId` enum is a realistic size.
- Local toolchain: rustc 1.97.0 (2026-07-07), while `rust-toolchain.toml` says `stable`.

---

## 1. Package recommendations

Action: **Add** (new to the workspace) / **Alter** (version or features) / **Swap** (replace a mechanism) /
**Remove** / **Keep** (evaluated, no change) / **Reject**. Licences are from [cr] unless marked.

### 1.1 Measurement and profiling (do first)

| Crate (pin) | Action | Fixes (finding) | Licence | Risk |
|---|---|---|---|---|
| `profiling = "1.0"` (1.0.18, already in the lock) | **Add** as a workspace dep; `profiling::scope!` per frame step and sub-phase | No frame timing anywhere; four hand-ordered loops (`player/src/main.rs:1424-1825`, `editor/src/play.rs:507-760`) [V] | MIT OR Apache-2.0 [cr] | Very low. Compiles to nothing without a backend feature |
| `tracy-client = "0.19"` (0.19.0) | **Add**, optional, behind a `profile-tracy` feature on player/editor/server → `profiling/profile-with-tracy` | Cross-thread timeline for rayon (`render/src/renderer/parallel.rs`), texture workers, upload thread | MIT/Apache-2.0 (the Tracy GUI is BSD-3, not linked) | Low. Builds C++ (the compiler is already needed for Luau). **Never on by default.** Keep it out of shipped players |
| `puffin = "0.20"` (0.20.0, MSRV 1.92) | **Reject for now** (revisit for the in-editor Profiler panel) | – | MIT OR Apache-2.0 | `puffin_egui` would tie in the egui version again; Tracy covers dev needs |
| `tracing` (0.1.44, already in the lock via winit/dioxus) | **Keep out of the engine API**; keep `log` | – | MIT | Higher per-span cost than `profiling` macros [I]; two logging systems |
| `criterion = "0.8"` (0.8.2, MSRV 1.86) | **Add** as a dev-dependency for micro-benchmarks: `World::property`, `descendants`, Lua `__index`, scheduler `take_due`, `Replicator::frame` | No sim micro-benchmarks [V✓]; baseline comparison for CI | Apache-2.0 OR MIT | Low. Heavier dependency tree (plotters) [verify]; dev-only |
| `divan` (0.1.21, last release 2025-04) | **Reject** in favour of criterion | – | MIT OR Apache-2.0 | Slower release cadence; no saved-baseline compare |

### 1.2 Scripting (Luau / mlua)

| Crate (pin) | Action | Fixes | Licence | Risk |
|---|---|---|---|---|
| `mlua = "0.12"` (0.12.2 = latest) | **Keep the version.** **Alter** features later: `["luau", "luau-jit"]` (L3) | Luau runs without codegen (`Cargo.toml:79` [V✓]); Roblox's guidance is selective `--!native` on numeric code [F] | MIT | Medium. Native code is skipped under breakpoints [F] and probably under the debugger step hook (`script/src/debugger.rs`) [I]. The watchdog interrupt in native code needs [verify]. Codegen only on x86-64/AArch64 [K], so **no wasm target**. Keep it opt-in per script (`--!native`) |
| `luau0-src = "=0.22.0"` | **Keep the exact pin.** Add a test that compiles and loads a chunk, plus a CI step that fails if `cargo update` moves it | Fragile mlua-sys ↔ luau0-src coupling (S §2; `Cargo.toml:77-84`) [V✓]. mlua-sys 0.13.0's `^0.22.0` range is wrong upstream [cr] | MIT | None. File an upstream issue so the next mlua-sys narrows the range; unpin only then |
| mlua native `vector` (`mlua::Vector`, 3-wide, no `luau-vector4`) | **Alter (L3)**: Vector3 as a native Luau vector with a type metatable for methods | `LuaVector3` userdata allocated per read (`script/src/types.rs:93, 926-960`) [V] | – | **High compatibility risk:** `typeof(v)` returns `"vector"`; `.X/.Y/.Z` vs `.x/.y/.z` field casing; `Magnitude`/`Unit`/`:Dot` need a metatable via the type-metatable API [verify mlua 0.12 API]. Needs a full luau-lsp definitions update (`script/src/definitions/`) |

### 1.3 Physics

| Crate (pin) | Action | Fixes | Licence | Risk |
|---|---|---|---|---|
| `rapier3d = { version = "0.36", features = ["parallel"] }` | **Alter, behind an `axcenity-physics/parallel` feature, after I1** | Single-threaded sim (S §3.10); no features on (`Cargo.toml:115` [V✓]) | Apache-2.0 [cr] | Medium. Gains come only with many independent islands [I]. Results may differ with thread count [verify]. Physics tests use tolerances, but determinism notes in `docs/architecture/physics.md` need updating. rayon is already in the tree |
| rapier3d `simd8` | **Defer / measure** | Same | – | Changes float results [I]; possible platform constraints [verify]. The real cost is the sync layer (S §3.2), not the solver |
| rapier3d `enhanced-determinism` | **Reject** | Cross-machine determinism is not a goal (S §3.7) | – | Slower (libm) |
| `avian3d` 0.8.0 | **Reject** | – | MIT OR Apache-2.0 | It is a Bevy plugin: it needs `bevy_ecs`/`bevy_app` and stores state in a Bevy World, so it would add a second world model |
| Jolt via Rust bindings (`jolt_rs` 0.1.2, ~10k downloads; others under 1k) | **Reject (revisit only if rapier proves to be the bottleneck after M3)** | – | Jolt is MIT; binding licences [verify] | No mature, maintained binding [cr]. Adds a C++ build plus an FFI safety surface (`unsafe_code = "deny"`) |

### 1.4 Assets, packs and file watching

| Crate (pin) | Action | Fixes | Licence | Risk |
|---|---|---|---|---|
| `notify = "8.2"` (8.2.2, MSRV 1.88; **not** 9.0.0-rc) | **Swap** the event source in `project::Watcher`; keep the stat-diff + 100 ms settle logic and fall back to `notify::PollWatcher` | 500 ms polling walk of `assets/`, scripts and CSS, up to 3 watchers (`project/src/watch.rs:24-27, 168-187`) [V✓] | **CC0-1.0** (Apache-compatible; no patent grant, so legal sign-off may want to note it) | Low–medium. Network filesystems and WSL lose events, hence the poll fallback. inotify is already in the tree via gilrs. Evaluate `notify-debouncer-full` 0.7.0 rather than hand-rolling the debounce (MIT OR Apache-2.0 [cr]) |
| `zstd = "0.14"` (0.14.3) | **Add** to `project` for `.axgame` per-entry compression (level ~19 at pack time) | `.axgame` is uncompressed, built fully in memory, unpacked to temp each launch (`project/src/packed.rs:11-13, 45+`) [V] | BSD-3-Clause (the zstd C library is BSD/GPLv2 dual; we take BSD) | Low. C build (cc already present). PNG/JPEG are already compressed, so gains are mostly FBX/JSON/Lua [I]. **Measure the ratio before shipping** |
| `lz4_flex` 0.14.0 | **Alternative** to zstd if a pure-Rust build matters more than ratio | Same | MIT | Low |
| `zip` (already a dep, deflate) | **Zero-new-dependency fallback** for packs | Same | MIT | Weaker ratio and speed than zstd [K] |
| `intel_tex_2 = "0.5"` (0.5.0) | **Add, editor/cook only** (L2): BC7/BC6H encoding of imported textures | RGBA8 only plus CPU box-filter mips at runtime (`render/src/renderer/gpu.rs:967-1000`); CPU RAM ≈ GPU RAM (R §3.5) [V] | MIT/Apache-2.0 | Medium. Uses `ispc_rt` prebuilt kernels [cr]; **check macOS arm64 and Windows prebuilts [verify]**. Must never enter the player (add a cargo-tree CI gate like the editor gate) |
| `ktx2 = "0.5"` (0.5.0) | **Add** (L2) as the cooked texture container reader in `assets` (player side) | Same | Apache-2.0 | Low. Pure-Rust reader. wgpu needs `TEXTURE_COMPRESSION_BC` (desktop OK; ASTC is mobile-only, not a current target) [K] |
| `texpresso` 2.0.2 | **Reject** | – | MIT | BC1–5 only, no BC7 |
| `basis-universal` 0.3.1 | **Reject** | – | MIT OR Apache-2.0 | Last release 2023-11; C++ build; transcoding not needed for desktop-only BCn |
| `postcard = "1.1"` (1.1.3) | **Swap** JSON → postcard for reliable net updates, snapshots and RemoteEvent payloads (M7); keep JSON behind a debug flag | JSON wire (`net/src/handshake.rs:118-119`, `wire.rs:1-3`); 5 MiB channel memory vs multi-MB snapshots (S §3.7) [V] | MIT OR Apache-2.0 | Medium. `Value` must stay serde-stable; this is a protocol version bump (handshake must reject mismatches) |
| `bincode` 2.x/3.0 | **Reject** | – | MIT | RUSTSEC-2025-0141 unmaintained [cr] |
| `rkyv` 0.8.18 | **Defer** to L2 (zero-copy cooked mesh cache); do not use for the wire | – | MIT | `unsafe` access patterns vs `unsafe_code = "deny"` [verify `rkyv`'s checked API cost]; format churn between majors |

### 1.5 Core data structures, hashing, concurrency

| Crate (pin) | Action | Fixes | Licence | Risk |
|---|---|---|---|---|
| Custom `core::Arena` | **Keep**; add `iter()`/`iter_mut()` and an O(1) `insert_at` (free-list bitmap) | No arena iterator (R §3.7); `insert_at` is O(free) (`core/src/arena.rs:133`) [V] | – | Low |
| `slotmap` 1.1.1 / `thunderdome` 0.6.1 | **Reject** | – | Zlib, MIT OR Apache-2.0 [cr] | Neither supports the replica ID space above 2³¹ with network-stable `insert_at` (`arena.rs`, `world.rs:25-47`). thunderdome's last release was 2023 |
| `string-interner` 0.20 / `ustr` 1.1.0 | **Reject for property names** (use a generated static `PropertyId`, pattern 2.3). **Maybe** `string-interner` later for attribute/tag names | String-keyed BTreeMap (`core/src/world.rs:55`) [V✓] | MIT/Apache-2.0; ustr is **BSD-2-Clause-Patent** (compatible, unusual) | A global interner (ustr) leaks and is process-wide, which is bad for multiple Worlds in the editor |
| `rustc-hash = "2"` (already a dep) | **Alter usage**: `FxHashMap` for engine-keyed long-lived maps: `World` side tables (`world.rs:272-275`), renderer caches (`renderer/types.rs:926-931`), `Streaming::hidden` (`scene/src/streaming.rs:46-48`) | Mixed hashing policy (R §3.4) [V] | MIT OR Apache-2.0 | Low. Keep std SipHash for keys from the network or user strings |
| `foldhash` 0.2 / `ahash` 0.8 | **Reject** as direct deps (already transitive) | – | Zlib / MIT OR Apache-2.0 | Adds nothing over rustc-hash for `u32`-pair keys [I] |
| `smallvec = "1"` (1.16.3, already transitive) | **Add, narrowly**: cached draw items per Instance (`SmallVec<[DrawItem;1]>`), event handler snapshots (`script/src/events.rs:211-245`) | Per-frame `Vec` churn (R §3.2-3.3, S §3.5) | MIT OR Apache-2.0 | Low. **Don't** use it for `Instance::children` before measuring (it makes Instance bigger) |
| `parking_lot` (0.12.5, already transitive) | **Reject for now** | `Assets` global `Mutex` (`assets/src/lib.rs:107-109`) [V✓] | – | The problem is *holding* the lock during decode, not lock speed. Fixed by M5 |
| `crossbeam-channel` | **Reject** | – | – | `std::sync::mpsc` is crossbeam-based since Rust 1.67 [K]; `crossbeam-deque` is already present via rayon |
| `rayon = "1.10"` (lock 1.12.0) | **Keep; widen use** (pattern 2.5): draw-list build, animator posing, audio sync over `&World` | Only `render` uses rayon (R §2) [V] | MIT OR Apache-2.0 | Determinism: collect results in index order (render already proves this pattern: `planning_comes_out_the_same_however_many_threads_share_it`) |
| `bevy_ecs` 0.20.1 (MSRV 1.95) | **Reject**, both as storage and as a scheduler | – | MIT OR Apache-2.0 | Its scheduler only orders systems over data in a bevy `World`/`Resources`. Using it means either moving the Instance tree into it (it then becomes the real world model, which breaks the Roblox-shaped tree and `BTreeMap` determinism) or wrapping everything in one `NonSend` resource (no parallelism gained). It breaks on every ~quarterly release. A ~150-line custom `Schedule` (pattern 2.1) covers the need |

### 1.6 Graphics stack, math, audio, net

| Crate (pin) | Action | Fixes | Licence | Risk |
|---|---|---|---|---|
| `wgpu = "30"` + `eframe = "0.36"` (0.36.2) + `egui_dock = "0.21"` + `egui-phosphor = "0.14"` + `glyphon = "0.12"`; winit stays 0.30 | **Alter, together** (M12) | 5 wgpu majors behind; editor/render lock-step (T §4.6) [V] | All MIT or MIT OR Apache-2.0 | **High churn**: 5 wgpu majors of API changes in `render` (34.7k LOC, 19 WGSL files); naga validation changes may reject shaders. Do it on a branch with GPU tests running on lavapipe (M10). **Don't** go to eframe 0.37 until egui_dock supports it |
| Decoupling the editor viewport from eframe's device | **Reject for now** | Same | – | A separate device needs texture sharing across devices (no portable wgpu path) or a CPU copy. Keep lock-step upgrades |
| `glam = "0.34"` (lock 0.34.1 → 0.34.3) | **Alter** (patch). Accept the ×5 duplicates for now | 5 glam copies (T §2.2) [V] | MIT OR Apache-2.0 | The duplicates come from rapier's `glamx` (0.33), kira and nalgebra conversion features. They can only be unified by following rapier's glam. Compile-time cost only [I]; don't fork |
| `kira = "0.12"` (→ 0.12.6) | **Keep**; patch bump | – | MIT OR Apache-2.0 | Fine for scope. The server-links-cpal issue is fixed by M8 (crate edges), not by swapping kira |
| `renet`/`renet_netcode = "2"` (→ 2.0.1) | **Keep**; patch bump. **Alter config**: `ServerAuthentication::Secure` + connect tokens (M11) | `Unsecure` on both sides (`net/src/transport.rs:240, 415`) [V] | MIT OR Apache-2.0 | A token issuer is needed (a dedicated server can mint them; LAN or editor hosting can stay unsecure, but say so in the UI) |

### 1.7 Build profiles and toolchain (Alter)

```toml
# rust-toolchain.toml: pin; bump deliberately in its own PR (CI runs -D warnings). Must be >= 1.95 for M12.
[toolchain]
channel = "1.97.0"            # [verify] = what CI's stable resolved to at the last green run
components = ["rustfmt", "clippy"]

# Cargo.toml
[profile.release]
lto = "thin"                  # cheap cross-crate inlining (hot path crosses core→scene→render)
codegen-units = 16            # default; keep release builds tolerable for devs
debug = "line-tables-only"    # usable Tracy/perf stacks and panic backtraces
# panic stays "unwind": assets loaders and editor Play rely on catch_unwind (assets/src/model.rs:550-610, editor/src/play.rs:1093)

[profile.dist]                # used by release.yml only
inherits = "release"
lto = "fat"
codegen-units = 1
strip = "debuginfo"           # keep symbols for crash reports [verify size budget]

[profile.dev.package.axcenity-script]
opt-level = 1                 # editor Play runs the Lua glue unoptimized today (S §3.6); 1 keeps rebuilds fast [I]
```
Risk: low. `fat` LTO plus `codegen-units = 1` lengthens release CI time considerably [I]; measure it. **Remove (crate edges, not
packages):** `script → render` (3 plain types), `script → audio` (`SoundAction`, enum lists), `script → mesh_edit` behind a
`csg` feature. That stops `server` linking wgpu, cpal/ALSA, image and meshopt (S §2) [I: confirm with `cargo tree -p axcenity-server`].

---

## 2. Patterns to port (picked 5, ordered by leverage per cost)

Why these: they target B's gaps #1, #2, #3, #4 and #5 directly. Each one builds on primitives Axcenity already has
(`revision`, `structure_version`, `ChangeLog`, `ItemChunk`, a `DrawList` boundary), so none of them needs an ECS rewrite.

### 2.1 Data-driven frame Schedule: Roblox RunService stages + Unity PlayerLoop
**Source.** Roblox task scheduler stages PreRender/PreAnimation/PreSimulation/PostSimulation/Heartbeat [F]
(create.roblox.com/docs/…/task-scheduler). Unity `PlayerLoop.GetCurrentPlayerLoop/SetPlayerLoop` [K].
**Fit.** It replaces four hand-ordered loops that have already drifted (render_step vs animators, audio and particle placement;
T §1.4 [V]). Creators already know the stage names. It is the prerequisite for spans (I1), stage parallelism (2.5) and moving
gameplay out of `script` (L7).
```rust
// crates/sim (new, Instance-aware, no render/egui deps): owns what every host steps today.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Stage { NetReceive, Input, PreSimulation, Simulation, PostSimulation, Heartbeat,
                 PreAnimation, Animation, PreRender, NetSend }
bitflags_like! { pub struct Hosts: u8 { PLAYER, EDITOR_PLAY, SERVER, TEST } } // tiny hand-rolled const set, no new dep
pub struct Sim { pub world: World, pub scripts: ScriptRuntime, pub physics: Physics,
                 pub characters: Controllers, pub agents: Agents, pub animators: Animators, /* ... */ }
pub struct FrameCtx<'h> { pub dt: f32, pub host: Hosts, pub io: &'h mut dyn HostIo } // input, sound sink, net
pub struct Step { pub name: &'static str, pub stage: Stage, pub hosts: Hosts,
                  pub run: fn(&mut Sim, &mut FrameCtx) }
pub struct Schedule { steps: Vec<Step> }
impl Schedule {
    pub fn standard() -> Self { Self { steps: crate::steps::STANDARD.to_vec() } } // the ONE order (frame-loop.md)
    pub fn insert_after(&mut self, anchor: &str, step: Step) { /* find by name, insert; panic if missing */ }
    pub fn names_for(&self, host: Hosts) -> Vec<&'static str> { /* used by the golden-order test */ }
    pub fn run(&mut self, sim: &mut Sim, ctx: &mut FrameCtx) {
        for step in self.steps.iter().filter(|s| s.hosts.contains(ctx.host)) {
            profiling::scope!(step.name);
            (step.run)(sim, ctx);
        }
    }
}
```
**Changes.** New `crates/sim`; `player/src/main.rs:1424-1825`, `editor/src/play.rs:507-760` (+ particles/streaming from
`viewport.rs:1257-1276`), `server/src/main.rs:175-208`, `testing/src/game.rs:129-230` become thin hosts. Overlay UI
assembly (`player/main.rs:1698-1789` ≈ `editor/viewport.rs:1299-1355`) becomes one `scene::overlay_items` function.
`docs/architecture/frame-loop.md` gains a generated table.
**Migration (tests stay green).** (1) Add `profiling` spans in the current loops (I1), so there is a before-picture. (2) Write `STANDARD`
by transcribing the player's order. Add a **golden test** asserting `names_for(host)` for each host *as each host behaves today*
(explicit, per-host exceptions for the editor drift). (3) Port `TestGame` (the smallest; fixed dt), then server, then player, each in its own
PR, with the full game-test suite (`axcenity-test examples/*`) green. (4) Port editor Play last. Then **remove the drift exceptions one at a time**
(each is a deliberate behaviour change with its own test: e.g. animators before render_step). This is the main risk: editor Play semantics change.

### 2.2 Dirty-driven render proxies: Godot servers/RIDs + Unreal FPrimitiveSceneProxy
**Source.** Godot: nodes hold RIDs, the scene layer pushes to RenderingServer, and bypassing it helps for "tens of thousands" of objects [F].
Unreal: per-primitive scene proxies updated only on dirty [K].
**Fit.** `build_draw_list` rebuilds every `DrawItem` every frame (`scene/src/draw_list.rs:449-529` [V✓]) although `revision` and
`structure_version` exist. `render` stays Instance-free (the cache lives in `scene`, which already depends on core). `ItemChunk` is the
retained template (`render/src/draw_list.rs:1164-1196`).
```rust
// crates/scene/src/retained.rs
pub struct DrawCache {
    entries: FxHashMap<InstanceId, Cached>,
    order: Vec<InstanceId>,            // deterministic emit order, rebuilt on membership change
    structure_version: u64,            // ancestor flags (in_union, stored, hidden-by-character) depend on tree shape
    assets_generation: u64,            // live reload / material files bump this (assets/src/reload.rs)
    meshes: FxHashMap<(u64, usize), Arc<ModelMesh>>, // keeps ModelMesh (and its LOD OnceLocks) across frames
}
struct Cached { revision: u64, items: SmallVec<[DrawItem; 1]> }
impl DrawCache {
    pub fn build(&mut self, world: &World, assets: &Assets, cam: Camera, list: &mut DrawList) {
        if world.structure_version() != self.structure_version || assets.generation() != self.assets_generation {
            self.refresh_membership(world);            // uses World class index (2.3); recompute ancestor flags once
        }
        for id in &self.order {
            let Some(inst) = world.get(*id) else { continue };
            let c = self.entries.get_mut(id).expect("membership");
            if c.revision != inst.revision() {        // physics-moved parts land here every frame: correct, bounded
                c.items.clear();
                emit_instance(world, assets, *id, &mut self.meshes, &mut c.items);
                c.revision = inst.revision();
            }
            list.items.extend(c.items.iter().cloned()); // step 2: Arc<[DrawItem]> chunks for static parts, no clone
        }
    }
}
```
**Pitfalls to encode as tests.** A MeshPart's SurfaceAppearance *child* changing doesn't bump the parent's revision. So either propagate
via ChangeLog (`ChildAdded`/property changes of appearance children → mark parent) or key on `structure_version` + child revision.
Streaming-hidden set changes (`scene/src/streaming.rs:93-112`), time-of-day independent items, and `Characters` decals must also invalidate.
**Changes.** `scene/src/draw_list.rs`, new `scene/src/retained.rs`; hosts hold a `DrawCache` (player `App`, editor viewport).
**Migration.** (1) Equivalence test: on randomised worlds plus edits, `DrawCache::build` output == `build_draw_list` output (sorted by
`draw_id`). (2) Ship behind a flag, with the existing `bench_10000_parts` and `bench_2000_mesh_parts` benches as the metric. (3) Static anchored parts go into
`ItemChunk`s by spatial cell (skip LOD resolve whole). (4) Remove the old path.

### 2.3 Per-class property slots + interned PropertyIds + class/tag index: Godot ClassDB/StringName, Unreal FProperty
**Source.** Godot ClassDB maps names → bound properties, StringName interning, Variant at the edge [K]. Unreal UHT static offsets [K].
Roblox `CollectionService:GetTagged` is indexed [K].
**Fit.** It keeps the dynamic Roblox surface (`Value` stays) but removes the string compares and ~2 KB BTree per Part (R §3.1), the
`String` per Lua access (`script/src/instance.rs:114-121` [V✓]), and ~15–20 full-tree walks per frame (S §3.1 [I count]).
```rust
// core/src/prop.rs, generated by a macro_rules! table next to class.rs (no proc-macro crate, no phf dep)
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)] #[repr(u16)]
pub enum Prop { Anchored, CastShadow, Color, Material, Orientation, Position, Size, Transparency, /* ~400 */ }
impl Prop {
    pub const fn name(self) -> &'static str { NAMES[self as usize] }
    pub fn from_name(s: &str) -> Option<Prop> { NAMES_SORTED.binary_search_by(|(n, _)| n.cmp(&s)).ok().map(|i| NAMES_SORTED[i].1) }
}
pub struct ClassLayout { pub props: &'static [Prop], slot_of: &'static [(Prop, u8)] } // sorted; ≤64 props per class
pub struct Instance { class: ClassName, /* name, parent, children, attributes, tags, prefab as now */
                      values: Box<[Value]>, dirty: u64 /* bit per slot since last consumer reset */, revision: u64 }
impl Instance {
    pub fn get(&self, p: Prop) -> Option<&Value> { self.class.layout().slot(p).map(|i| &self.values[i]) }
    #[deprecated(note = "use get(Prop::…) on hot paths")]
    pub fn property(&self, name: &str) -> Option<&Value> { Prop::from_name(name).and_then(|p| self.get(p)) }
}
// core/src/index.rs, maintained in World::create/set_parent/destroy/add_tag/remove_tag
pub struct Index { by_class: Vec<BTreeSet<InstanceId>>,   // BTreeSet: deterministic order (physics relies on it)
                   by_tag: HashMap<Box<str>, BTreeSet<InstanceId>> }
impl World { pub fn all_of(&self, c: ClassName) -> impl Iterator<Item = InstanceId> + '_ { /* + IsA via class predicates */ }
             pub fn tagged_iter(&self, tag: &str) -> impl Iterator<Item = InstanceId> + '_ { /* O(k) */ } }
```
The `dirty` bitset also enables a *motion-only* fast path for 2.2 (Position/Orientation changed → patch the transform only) and a separate
motion revision for replication (S §3.7).
**Changes.** `core/src/world.rs`, `class.rs`, new `prop.rs` and `index.rs`; callers migrate gradually (~500 sites). In `script/src/instance.rs`, `__index`
takes `mlua::String` and does `to_str()` (borrowed, no alloc), then `Prop::from_name`, then the special-case chain only on a miss. The world JSON format is unchanged (names).
**Migration.** Index first (M2: additive, no API break, removes the walks). Then slots behind the unchanged `property(&str)` shim, with property tests
(`set_property`/`property` round-trip for every class × every default). Then a codemod of hot paths (scene, physics, net) to `get(Prop::X)`.
Undo snapshots (`World::clone`, R §3.7) get cheaper as a side effect.

### 2.4 Import pipeline + threaded loading: Godot `.import` sidecars, `uid://`, `ResourceLoader.load_threaded_*`
**Source.** Godot import sidecar + UID with path fallback + engine-native cached output [K]. `load_threaded_request/get_status/get` [S].
"Load on a thread, add on the main thread" [F]. Roblox `ContentProvider:PreloadAsync` [K] as the Luau-facing name.
**Fit.** It removes the main-thread decode under the global lock (`assets/src/lib.rs:107-109, 236-264` [V✓]) and the player's lack of preload
(R §3.5), and gives a home for meshopt LODs, BC7 and mips. It keeps the plain-folder project (sources stay; outputs go to `.axcenity/imported/`, which is already per-machine).
```rust
// assets: phase 1 (M5): non-blocking API, work outside the mutex
pub enum Slot<T> { Loading(LoadTicket), Ready(Arc<T>), Failed(Arc<str>) }
impl Assets {
    pub fn request_model(&self, key: &str) -> LoadTicket;           // insert Loading under lock, release, decode on worker pool
    pub fn model_ready(&self, key: &str) -> Option<Arc<Model>>;      // never blocks; draw list skips/placeholder until Ready
    pub fn model_blocking(&self, key: &str) -> Option<Arc<Model>>;   // physics hulls, tests: waits on the ticket, not the lock
    pub fn progress(&self) -> (usize /*done*/, usize /*requested*/);
    pub fn generation(&self) -> u64;                                 // bumps when anything becomes Ready (feeds DrawCache)
}
// project/assets: phase 2 (L2): sidecar  models/ship.glb.import.json
#[derive(Serialize, Deserialize)]
pub struct ImportMeta { pub uid: AssetUid /* u128, "ax://<base32>" */, pub importer: Importer,
                        pub params: ImportParams /* lods, bc7, max_size, srgb */, pub source_hash: u64 }
```
**Changes.** `assets/src/lib.rs` (lock scope), `scene/src/draw_list.rs:1070` (`model()` → `model_ready()`), player loading screen
(`scene::asset_files` today editor-only, `editor/src/loading.rs:221`), `script` (`ContentProvider:PreloadAsync`), later `project` (sidecars, cook in
`pack_game`), `render` (upload BCn: `TextureSource` gains a format field, still plain data).
**Migration.** Phase 1 is invisible to content: `model()` keeps its blocking semantics as `model_blocking`. Flip call sites one by one; an asset test asserts
that two concurrent loads don't serialise (a timing-free test: a worker holds a model decode while another thread's `texture_ready` returns). Phase 2
adds sidecars lazily (created on first import; missing = path fallback), so existing projects load unchanged. Add a manifest migration only when refs
switch to UIDs.

### 2.5 Parallel read, serial write: Roblox Parallel Luau + Unreal Pipes, via rayon (an extension of 2.1)
**Source.** Parallel Luau: "Instance writes must happen serially", desync/sync phases [F]. UE Pipes serialise access to one resource [F].
**Fit.** Rust's `&World` vs `&mut World` already enforces it. Add a `Step::parallel_read: Option<fn(&Sim) -> Box<dyn Apply>>` variant: compute on rayon
over `&World`, then apply writes serially in index order (determinism preserved). First users: draw-list emit per `order` chunk (2.2), animator posing for
many characters, audio sync diff, nav fingerprint. **Not** Parallel Luau Actors yet: they need multiple `Lua` states, which strains "one runtime" and the
sandbox. **Pick it last:** it needs I1 numbers to choose targets and 2.1–2.3 to exist.

Patterns considered and not picked: ECS/Mass for the Instance tree (DOTS fragmentation evidence [F], B §1.2); a render graph (B ranks it a
maintainability gap; draw recording is 1.2 ms per the perf doc); GDExtension-style native plugins (premature before the crate splits).

---

## 3. Refactoring roadmap

Effort is for one developer: **S** ≤ 3 days, **M** 1–3 weeks, **L** 1–3 months. "Metric" says what I1 should show moving.
The standard scenes are **S10k** (10k parts: 9k anchored, 1k unanchored falling, 200 scripts with Heartbeat and `task.wait` loops, 50 Sounds,
20 animated characters, 10 nav agents) and the existing **render benches**.

### 3.1 Immediate (days)

**I1. Measure first: headless sim benchmark + profiling spans.** *Files:* new `testing/benches/sim.rs` (or `player/src/bench_tests.rs`
style, `#[ignore]`), workspace `profiling` dep, `profiling::scope!` in all four hosts' steps, `build_draw_list` sub-phases, `ScriptRuntime::step`,
`Physics::step` (sync / substeps / write-back), `Replicator::frame`; `profile-tracy` feature; JSON output (stage → median ms). criterion micro-benches
for `property()`, `descendants()`, Lua `part.Position` get/set, `take_due`. *Effort* M−(5–7 days). *Benefit:* turns every [I] below into numbers;
a regression baseline. *Risk:* low. Bench variance on shared CI runners means record-only nightly, not gating. *Deps:* none. **Verdict: do first; everything perf-related waits on it.**

**I2. Fix Instances as Lua table keys.** Test first: `local t = {}; t[workspace.Baseplate] = 1; assert(t[workspace.Baseplate] == 1)`, plus
`rawequal`, plus `data[player]` across `PlayerAdded`. Fix: one weak-valued registry table `id(u64) → userdata` (`__mode = "v"`); every
`LuaInstance` creation (`script/src/types.rs:954-958`, `instance.rs:150, 246`) goes through `instance_value(lua, id)`. Generational IDs make stale
reuse safe. *Effort* S (1–2 d). *Benefit:* correctness (a common Roblox idiom silently broken [I]), plus fewer userdata allocs per access. *Risk:* low.
Keep per-VM tables (server and client `Context`s are separate VMs). *Verdict:* **do now.**

**I3. Fix stale docs.** `docs/development.md:39` ("CI is off" vs `.github/workflows/ci.yml` push/PR triggers. Confirm in repo settings whether
Actions is disabled). `docs/architecture/frame-loop.md:13` (editor Play *does* network-receive/send over loopback, `editor/src/play.rs:529-549, 754-758`,
and its order differs from the player's in 3 places; document that until 2.1 lands). `docs/architecture/crates.md:30` (add `net`). *Effort* S (hours). *Verdict:* do now.

**I4. Cheap `with_world`.** `script/src/lib.rs:2371-2378` builds a full `World::new()` (13 services) on every call via `mem::take`. Use
`lua.set_app_data(Some(world))` with `Option<World>` and `take()`, or `World::placeholder()` (no services). *Effort* S (≤1 d). *Benefit:* tens of µs/frame [I]
(I1 shows it). *Risk:* low. *Verdict:* do.

**I5. Toolchain pin + release profiles** (§1.7). *Effort* S. *Benefit:* CI stops breaking on new lints; ~5–15 % CPU from thin LTO [I, typical, measure
with I1]. *Risk:* low (release CI time). *Verdict:* do.

**I6. Luau pin guard + patch bumps.** A test that loads a compiled chunk; a CI check that `luau0-src` resolves to `=0.22.0`; `cargo update -p glam -p kira
-p renet -p renet_netcode`, then `cargo hakari generate`. *Effort* S. *Verdict:* do.

**I7. Nav hitch.** `nav/src/lib.rs:410-455`: fingerprint anchored geometry only, `skip` as a set, no `format!("{:?}")`. *Effort* S (1–2 d). *Benefit:* removes the
periodic ~50 ms rebuild every 0.5 s when props move [I] (metric: max frame time on S10k with agents). *Risk:* agents won't path around moving
unanchored props (they already can't reliably). *Verdict:* do.

**I8. MeshPart per-frame allocations.** Cache `Arc<ModelMesh>` per (model key, primitive) across frames (`scene/src/draw_list.rs:1120, 1372-1380`),
so LOD `OnceLock`s survive; reuse `Skin` buffers. *Effort* S–M (2–4 d). *Metric:* `bench_2000_mesh_parts` build_draw_list ms. *Risk:* low. Invalidate on
asset reload key. *Verdict:* do (it is a stepping stone to 2.2).

**I9. Non-allocating `descendants_iter()` + `FxHashMap` for engine-keyed maps.** `core/src/world.rs:1194-1208`, `272-275`, `render/src/renderer/types.rs:926-931`,
`scene/src/streaming.rs:46-48`. Migrate the per-frame call sites (S §3.1 table). *Effort* S–M (3–5 d). *Benefit:* fewer allocs; modest [I]. *Verdict:* do.
It shrinks M2's diff.

**I10. `notify` watcher** (§1.4). `project/src/watch.rs`. *Effort* S–M (3–4 d incl. 3-OS manual test). *Benefit:* reload latency 0.5–0.6 s → ~0.1 s; no
background stat walk. *Risk:* platform event quirks, so keep the poll fallback. *Verdict:* do, low priority within the tier.

**I11. Player background texture loading + preload.** Call `load_textures_in_background` in the player (only the editor does, `editor/src/app.rs:1183`), and
drive the existing loading screen from `scene::asset_files`. *Effort* S–M (2–4 d). *Metric:* first-frame and max-frame ms on hello-pbr / bench project.
*Risk:* textures pop in. *Verdict:* do; it is the cheap half of 2.4.

### 3.2 Medium-term (weeks)

| # | What (pattern) | Files | Effort | Benefit / metric | Risk | Deps | Verdict |
|---|---|---|---|---|---|---|---|
| M1 | Shared `Schedule` + `sim` crate (2.1) | new `crates/sim`; 4 hosts; `frame-loop.md` | M (2–3 wk) | Fixes host drift (correctness); spans per stage; enables M3/L4/L7. Metric: host-order golden tests; no ms change expected | Medium: editor Play order changes | I1 | **High value, do** |
| M2 | Class/tag index + per-service `structure_version` (2.3 part 1) | `core/src/world.rs`, new `index.rs`; physics, audio, anim, nav, procedural, script call sites | M (1–2 wk) | Removes ~15–20 O(N) walks/frame [I]. Metric: ms in non-render stages on S10k | Low–medium: index consistency bugs; add debug-assert cross-check vs walk | I9 | **Highest leverage, do** |
| M3 | Physics sync overhaul | `physics/src/lib.rs:535-975` | M (2–3 wk) | Physics-owned revision so own writes aren't re-read; added/removed lists instead of global resync; wake neighbours not `wake_up_all` (`:975`); write-back once per frame (check movers/joints read World mid-substep first); then try `parallel` | Medium: behaviour drift in vehicles/joints; game tests guard | M2, I1 | Do; **measure the `parallel` feature separately** |
| M4 | Retained draw list (2.2) | `scene/src/draw_list.rs`, new `retained.rs`, hosts | M (2–4 wk) | Metric: `bench_10000_parts` build_draw_list 3.3 ms → target <1 ms static [I] | Medium: missed invalidation (stale visuals). Equivalence tests | M2, I8 | **Do** |
| M5 | Assets off-lock + threaded loading API + `ContentProvider:PreloadAsync` (2.4 phase 1) | `assets/src/lib.rs`, `scene`, `script`, player loading | M (2 wk) | Metric: max frame ms on first sight of a big FBX; no hitch | Medium: pop-in, placeholder policy | I11 | Do |
| M6 | Script runtime hot paths | `script/src/instance.rs`, `scheduler.rs:101-113`, `events.rs:211-245`, `context.rs:353-391` | M (2–3 wk) | `mlua::String` keys; member table per class; binary-heap waits; prune connections on Destroy; compiled-chunk cache by source hash; thread reuse **only** for handlers proven not to yield [verify semantics vs Roblox]. Metric: S10k script stage ms; criterion `part.Position` ns | Medium: subtle coroutine semantics | I1, I2 | Do, item by item |
| M7 | Replication: motion revision + postcard wire | `net/src/replicate.rs:233-249, 411-528`, `handshake.rs`, `wire.rs` | M (1–2 wk) | No full `Sent` clone per moving part per frame; smaller snapshots. Metric: server ms/frame with 8 loopback clients; snapshot bytes | Medium: protocol bump | M2 (dirty bits help) | Do |
| M8 | Cut `script → render/audio/mesh_edit` edges | move `GraphicsSettings`, `Capabilities`, `Camera`, `SoundAction` to `core` or `*-types`; `csg` feature | S–M (1 wk) | Server stops linking wgpu/cpal [I]; smaller rebuild fan-out. Metric: `cargo tree -p axcenity-server`, clean build time | Low | – | **Cheap, do early** |
| M9 | `.axgame` zstd + read-in-place (no temp unpack) | `project/src/packed.rs`, `player/src/main.rs:95-103`, `server/main.rs:88-96` | M (1 wk) | Metric: pack size, launch time | Low | – | Do after measuring the ratio |
| M10 | CI: lavapipe GPU tests, nightly I1 benches, `axcenity-test` + server headless, ship server; player no-intel_tex gate | `.github/workflows/*.yml` | S–M (1 wk) | Real render coverage; perf trend | Low | I1 | Do (gates M12) |
| M11 | renet secure mode for dedicated servers | `net/src/transport.rs:240, 415` | M (1–2 wk) | Encrypted, token-gated connections | Medium: token issuance UX | – | Do **before** any internet hosting claim |
| M12 | wgpu 30 / egui 0.36 stack upgrade (§1.6) | `render` (all), editor, `Cargo.toml` | M–L (2–4 wk) | Unblocks newer wgpu features | **High**: API and naga churn | M10, I5 (MSRV ≥1.95) | Do, but not before M10 |
| M13 | Nav build on worker + one shared navmesh cache | `nav/src/agents.rs:335-357`, `script/src/humanoid.rs:638-655` | S–M (1 wk) | Removes the remaining build hitch; no double builds | Low | I7 | Do |
| M14 | UI layout cache (reuse `TaffyTree`, style cache, one layout per editor frame) | `ui/src/lib.rs:1160-1164`, `editor/viewport.rs:1052, ~1087`, `player/main.rs:901-925` | M (2 wk) | Metric: UI ms with a 500-element inventory GUI | Medium | I1 | Do if I1 shows UI >1 ms |

### 3.3 Long-term (months)

| # | What | Effort | Benefit / metric | Risk | Deps | Verdict |
|---|---|---|---|---|---|---|
| L1 | Per-class slots + `Prop` ids + dirty bits (2.3 part 2) | L (1–2 mo) | Memory per Part ~2 KB → ~0.5 KB [I]; property read ns; cheaper `World::clone` for undo/Play | Medium–high: ~500 call sites; reflection consumers (properties panel, world_file, save, net) | M2 | Do **after** M2/M4 numbers confirm property access still dominates |
| L2 | Import pipeline: sidecars + UIDs + cooked meshes (meshopt LODs offline) + BC7/KTX2 + offline mips (2.4 phase 2) | L (1.5–2 mo) | VRAM ~4× less for BC7 vs RGBA8 [K]; no runtime FBX parse in shipped games; rename-safe refs | Medium: two content paths during transition; ispc prebuilts [verify] | M5, M9 | Do; biggest shipped-game win |
| L3 | Luau native `vector` for Vector3 + opt-in `luau-jit` (`--!native`) | L (1 mo+) | Metric: numeric script micro-benches; allocs per `part.Position + v` | **High** compat (`typeof`, casing, definitions, blocks translator) | M6 | Do `luau-jit` opt-in first (cheap); `vector` only with a compat test suite |
| L4 | Parallel read / serial write stages (2.5) | L (1–2 mo, incremental) | Metric: S10k frame ms on 8+ cores | Medium: determinism (ordered apply) | M1, M4 | Do, targeted by I1 data |
| L5 | GPU-driven path: shared mesh arena, persistent instance buffers for static items, slim 88 B shadow stream, `multi_draw_indexed_indirect` | L (2–3 mo) | Metric: render CPU ms on forest/terrain benches (9.5–10 ms Ultra per perf doc) | Medium: wgpu feature availability per backend [verify on Metal] | M12 | Later; after M4 removes the scene-side cost |
| L6 | Split editor into sub-crates behind `EditorContext` (animation, model_studio, ui_designer, block_editor first, ~50k LOC) | L (2–3 mo, incremental) | Incremental rebuild 5–6 s → target ~2 s [I]; enforced boundaries | Medium: 109-field `EditorApp` | – | Do opportunistically, one panel family per PR |
| L7 | Extract gameplay systems from `script` into `gameplay` crate (`GameSystem` trait: register Lua API + schedule steps) | L (1–2 mo) | `ScriptRuntime` ~40 fields → VM core; `net` talks to a message interface not ~30 methods | Medium | M1, M8 | Do after M1 proves the Schedule |
| L8 | Render graph (transient aliasing, pass culling) | L | Maintainability | Low urgency | M12 | **Defer** |

**Dependency spine:** I1 → (I2–I11 in parallel) → M8, M2 → M1, M3, M4 → M5/M7/M6 → L1, L4, L7; M10 → M12 → L5; M5 + M9 → L2.

### 3.4 Cost-vs-benefit summary
- **Best ratio:** I2 (a correctness bug, 1–2 d), I1 (it unblocks every decision), M2 (one index, ~7 crates benefit), M8 (1 wk, server stops linking GPU/audio),
  I7 (a 50 ms hitch for ~1 d [I]).
- **High benefit, high cost, worth it:** M1 (correctness across 4 hosts), M4, L2.
- **Uncertain; measure before committing:** rapier `parallel`/`simd8`, `luau-jit`, L1 slots (M2 + M4 may already remove most property-read volume), M14.
- **Rejected:** bevy_ecs, avian3d, Jolt bindings, bincode, basis-universal, texpresso, divan, slotmap/thunderdome, ustr, parking_lot swap, viewport device decoupling, eframe 0.37 (until egui_dock catches up), `panic = "abort"`.
