# Phase 1 audit, part B: simulation and runtime crates

Scope: `physics`, `script`, `audio`, `net`, `anim`, `character`, `nav`, `save`, `procedural`, `mesh_edit` (brief).
Source tree: `/home/user/axcenity-engine` (read-only). All paths below are relative to `crates/` unless they start with `docs/`.

Labels used in this report:
- **[V]** verified: I read the code at the cited line.
- **[I]** inference: follows from the code but was not measured or run. `cargo` could not resolve crates offline, so nothing was built or benchmarked.

---

## 1. Subsystem map

### Lines of code (`wc -l`, all `.rs` files under `src/`, tests included)

| Crate | Total LOC | Of which tests (approx.) | Largest non-test files |
|---|---|---|---|
| `script` | 49,484 | ~13,500 | `definitions/docs.rs` 5,438, `definitions/tables.rs` 3,956, `lib.rs` 2,532, `types.rs` 1,234, `dialogue.rs` 1,214, `instance.rs` 1,152, `context.rs` 1,020, `humanoid.rs` 1,010 |
| `mesh_edit` | 22,754 | ~6,000 | `paint.rs` 1,107, `rig.rs` 866, `ops.rs` 830, `model_doc.rs` 705 |
| `anim` | 11,653 | ~2,400 | `animators.rs` 1,346, `files.rs` 1,337, `controller.rs` 869, `ragdoll.rs` 844, `sequence.rs` 813 |
| `physics` | 8,408 | ~3,100 | `lib.rs` 1,444, `query.rs` 735, `joints.rs` 704, `shapes.rs` 641, `movers.rs` 476, `vehicles.rs` 440 |
| `net` | 8,231 | ~2,600 | `game.rs` 1,278, `replicate.rs` 705, `transport.rs` 580, `streaming.rs` 543, `motion.rs` 540 |
| `nav` | 4,199 | ~1,700 | `agents.rs` 802, `behavior.rs` 634, `lib.rs` 483 |
| `character` | 4,082 | ~2,000 | `lib.rs` 1,547, `seat.rs` 433 |
| `audio` | 2,942 | ~530 | `lib.rs` 1,371, `effects.rs` 586, `mix.rs` 455 |
| `save` | 2,332 | ~960 | `lib.rs` 631, `state.rs` 581 |
| `procedural` | 1,520 | ~390 | `lib.rs` 420, `surface.rs` 216 |

About 9.4k of `script`'s lines are the luau-lsp definitions and API docs (`definitions/`), which are data. The runtime itself is roughly 26k lines.

### `physics`: rapier3d integration
- **`lib.rs`** defines `Physics`, which owns a rapier `PhysicsWorld` and maps `InstanceId -> Body` in a `BTreeMap` so iteration order is deterministic (`physics/src/lib.rs:117-125`).
- `step()` (`lib.rs:535`) runs this sequence:
  1. `sync_from_world`, which is the World -> rapier sync.
  2. `sleep_far_bodies`.
  3. Read gravity and wind.
  4. A fixed-step accumulator loop at 60 Hz, with at most 10 steps (`MAX_STEPS_PER_FRAME`, `lib.rs:93`). Each step runs, in order: wind, joint drive, movers, vehicles, `rapier.step_with_events`, breaking, `write_to_world` (rapier -> World), wheels, touches, shatters, fracture aging.
- `assembly.rs`: welds are grouped by union-find, and each assembly becomes one rigid body.
- `joints.rs`: constraints become rapier impulse joints, plus break detection.
- `movers.rs`: LinearVelocity, AlignPosition and similar, re-read from the World every frame. They keep no rapier state.
- `vehicles.rs`: rapier `DynamicRayCastVehicleController`, rebuilt every frame from the World.
- `fracture.rs`: impact shatter. `shapes.rs`: Part, MeshPart and Union colliders (hulls and convex decomposition). `material.rs`: a contact hook for weighted friction. `query.rs`: raycast, shapecast and overlaps, with colliders built on demand and limited to a `Reach`.
- Data flow: the World holds Part properties. Physics detects changes by comparing each part with what it last saw (`PartState`); there are no events. It writes `Position`, `Orientation` and the velocities back through `World::set_property`, which records `core::Change`s that scripts later turn into `Changed` events.

### `script`: Luau runtime, the engine API, and most gameplay systems
- **`ScriptRuntime`** (`lib.rs:222`) holds **two Luau VMs** (a `server` and a `client` `Context`), a `Transport` that carries RemoteEvents between them, and many gameplay sub-states: health, abilities, dialogue, quests, prompts, clicks, tools, humanoids, players, saves, levels, sequences, spline followers, procedural `Generators`, volume sliders, and the `started` and `clones` script lists.
- **`context.rs`**: one VM.
  - `Context::new` sets up the libraries, about 30 `app_data` slots (`context.rs:52-67`), the sandbox and the watchdog.
  - `load_script` compiles source as text only (`context.rs:353-369`).
  - `Context::step` (`context.rs:375`) prunes connections and then, inside `with_world`, runs tween step, `scheduler::step`, csg step, debris and decals.
- **World handoff**: `with_world` (`lib.rs:2371`) moves the whole `World` into Lua app data with `std::mem::take` for the duration of each call into Lua, then moves it back.
- **`instance.rs`**: `LuaInstance(InstanceId)` is a userdata holding only an ID. `__index` and `__newindex` take `key: String` and run a long chain of string-matched special cases before the generic property lookup (`instance.rs:114-121`, `get` at `:136-249`, `set` at `:251-424`).
- **`scheduler.rs`**: a `Vec<Waiting>` of `(due, order, thread, resume_with, script)`. `step` partitions the vector, sorts what is due, and resumes each through `resume_as` -> `resume` (watchdog, debugger, `thread.resume`).
- **`events.rs`**: `Connections` is a `BTreeMap<(Owner, Event), Vec<Handler>>`. Each fire clones the handler list, then calls `lua.create_thread` per handler (`events.rs:211-245`).
- **`changes.rs`**: turns the World's change log into Changed, PropertyChanged, ChildAdded, DescendantAdded and AncestryChanged events.
- **`sandbox.rs`**: each script gets its own environment table, built from an allowlist plus copies of the libraries (`sandbox.rs:220-261`). `lua.globals()` is never exposed. **`watchdog.rs`** is a Luau interrupt that reads the clock every 1000 interrupts and gives each resume a 3 s budget. **`debugger.rs`** is the only `unsafe` code (step hook).
- API modules (`tween`, `humanoid`, `players`, `tool`, `prompt`, `health`, `ability`, `dialogue`, `quest`, `sequence`, `camera`, `input`, `remote`, `network`, `ownership`, `csg`, `query`, `physical` and others) each own a slice of gameplay and are driven by `ScriptRuntime::step_*` calls from the host.

### `audio`: Kira playback
- `GameAudio::update(actions, world, assets, listener)` (`audio/src/lib.rs:166`) starts the Kira `AudioManager` lazily, applies the queued `SoundAction`s, then `sync` brings every Sound Instance in line with Kira (`lib.rs:668-685`).
- `mix.rs`: SoundGroups as Kira sub-tracks, plus ReverbZones. `effects.rs`: SoundEffect chains.
- Spatial audio uses Kira's listener and emitters, with the camera as listener.

### `net`: renet multiplayer
- `transport.rs` runs renet over UDP with netcode, or over an in-memory loopback with simulated delay, jitter and loss.
- `session.rs` handles the host and join sessions. `handshake.rs` holds the messages, encoded as **JSON** (`handshake.rs:118-119`). `discovery.rs` is LAN discovery.
- `replicate.rs`: `Replicator::frame` compares the world against what was last sent and builds an `Update` of Create, Change and Destroy ops.
- `motion.rs`: per-player `MotionSender` sends quantized 32-byte part states on the unreliable channel, prioritized and within a byte budget. `Smoother` interpolates them on the receiving side.
- `ownership.rs`: client-owned characters and assemblies, plus the server's `CharacterCheck` speed and teleport validation. `streaming.rs`: interest management for StreamingEnabled.
- `game.rs`: `Network::receive` (frame step 0) and `Network::send` (frame step 6) are the glue between the session and `ScriptRuntime`.

### `anim`
- `Animators::update` (`anim/src/animators.rs:288`) runs every Animator, AnimationTrack, Ragdoll and IKControl in the Workspace, poses skeletons (`Poses`) and fires events.
- `controller.rs` holds the state machines and blending, `clip.rs` and `property_clip.rs` sample clips, and `ik.rs`, `ik_control.rs` and the feet code handle IK.
- `sequence.rs` covers cutscenes, `ragdoll.rs` builds ragdolls, and `files.rs` handles the JSON formats.
- It is plain CPU work with no third-party crates beyond glam and serde.

### `character`
- Kinematic controllers. `Controllers::step` (`character/src/lib.rs:407`) handles walking, running, jumping, crouching and slopes using `physics::raycast` probes.
- `keep_kinematic` anchors the controller's parts before the physics step. `seat.rs` handles seats.

### `nav`
- `NavMesh::build` (`nav/src/lib.rs:109`) builds a column or heightfield mesh from `physics::solids`. `path.rs` does A*.
- `agents.rs`: `Agents::step` drives NavigationAgents. `perception.rs` covers sight and hearing. `behavior.rs` runs behavior trees from JSON.
- The mesh is rebuilt when `fingerprint(world)` changes, at most every 0.5 s (`agents.rs:36`, `:335-357`).

### `save`
- `Saves`, slot files, checkpoints, and `state.rs`, which captures and restores world state as JSON (`save/src/state.rs:112-188`, `restore` at `:323`).
- It is not on the per-frame path, except that `ScriptRuntime::step` flushes player data at most once a second (`script/src/lib.rs:1243-1258`).

### `procedural`
- `Generators::update` (`procedural/src/lib.rs:68`) rebuilds SplineMesh and SplineScatter output whose spline or settings changed. It is called from `ScriptRuntime::step` on every device.

### `mesh_edit` (brief)
- CPU mesh tooling: plane cut, Voronoi fracture, CSG booleans, the half-edge `EditMesh` for Model Studio, modifiers, UV unwrap, painting, rigging and baking.
- At runtime it is used only through `script/src/csg.rs` (`UnionAsync` on a `std::thread`, `csg.rs:153`). It is mostly an editor crate that `script` pulls into every runtime binary.

---

## 2. Dependency and provider matrix

Versions are pinned in the root `Cargo.toml` `[workspace.dependencies]`; resolved versions are from `Cargo.lock`.

| Crate | Third-party deps (workspace pin -> lock) | What it powers | Hook into the main loop (player `crates/player/src/main.rs`) |
|---|---|---|---|
| physics | `rapier3d = "0.36"` -> 0.36.1 (brings parry3d 0.31.1 and nalgebra 0.35). **No features enabled**: no `parallel`, no `simd-*`, no `enhanced-determinism` [V] (`Cargo.toml:115`) | Rigid bodies, colliders, joints, ray-cast vehicle, queries | Step 3: `self.physics.step(&mut self.world, seconds)` at `main.rs:1570` (and `:1561` for client-owned assemblies). Touches are taken at `:1577`. `dev` profile builds physics at opt-level 3 (`Cargo.toml:170`) |
| script | `mlua = { 0.12, features=["luau"] }` -> 0.12.2 / mlua-sys 0.13.0. `luau0-src = "=0.22.0"` -> 0.22.0+luau740 (build-dep only). `glam 0.34`, `serde_json 1` | Luau VM (two per runtime), sandbox, scheduler, the whole engine API | Step 2: `scripts.step` at `main.rs:1543` and `stepped` at `:1545`. Step 4: `fire_touched` `:1578`, `heartbeat` `:1589`. Also `render_step` `:883`, and `step_humanoids` and the other `step_*` calls around `:1505` |
| audio | `kira = "0.12"` -> 0.12.5 (default features, so it **includes `cpal`**, lock line ~3044), `mint 0.5`, `thiserror 2` | Playback, buses, effects, spatial audio | After step 4: `apply_audio` -> `audio.update(actions, world, assets, listener)` at `main.rs:440`, called from around `:1592` |
| net | `renet = "2"`, `renet_netcode = "2"` (2.0.0), `serde`, `serde_json`, `log` | UDP transport with netcode handshake and timeouts; JSON payloads | Step 0: `network.receive` at `main.rs:1455`. Step 6: `network.send` at `:1796` |
| anim | `glam`, `serde`, `serde_json` | Clips, state machines, IK, ragdolls, sequences | Step 4: `animators.update` at `main.rs:1619` |
| character | none (engine-internal only) | Controllers | Step 1: `characters.step` around `main.rs:1496-1501`, `keep_kinematic` at `:1566` |
| nav | `serde`, `serde_json` | Navmesh, agents, behavior trees | Step 1: `agents.step` at `main.rs:1515`, `keep_kinematic` at `:1567` |
| save | `serde`, `serde_json`, `thiserror` | Slots, snapshots | Through `script` (`SaveService`), with a flush in `ScriptRuntime::step` |
| procedural | `glam` | Spline generators | Inside `ScriptRuntime::step` (`script/src/lib.rs:1268`) |
| mesh_edit | `glam`, `image 0.25`, `meshopt 0.7`, `serde`, `serde_json` | Mesh tooling | Only from scripts' `UnionAsync` (on a thread) and the editor |

**The luau0-src pin [V]** (`Cargo.toml:77-84`, `script/Cargo.toml` `[build-dependencies]`)
- mlua-sys 0.13.0 can read Luau bytecode only up to version 11.
- luau0-src 0.22.1 (Luau 0.744) writes version 13. With it, mlua treats every compiled chunk as source text and cannot load it.
- So `luau0-src` is pinned with `=0.22.0` (Luau 0.740). The comment says to raise it only together with `mlua`.
- This is a fragile coupling. Any `cargo update` or `hakari` change that bumps one without the other breaks script loading. `script` has no `build.rs`; the build-dependency exists only to force the resolver.

**Engine-internal fan-in of `script`** [V] (`script/Cargo.toml`): `anim`, `assets`, `audio`, `character`, `core`, `mesh_edit`, `nav`, `physics`, `procedural`, `render`, `save` (11 engine crates).

How much `script` actually uses each (count of `axcenity_X::` references outside tests):

| Crate | References | Files |
|---|---|---|
| physics | 26 | 11 |
| character | 24 | 8 |
| anim | 16 | 7 |
| render | 12 | 4 (`GraphicsSettings`, `Capabilities`, `Camera`, and glam re-exports) |
| audio | 12 | 6 (`SoundAction`, `mix`, `REVERB_TYPES`) |
| save | 5 | 4 |
| nav | 2 | 1 |
| procedural | 2 | 1 |
| mesh_edit | 1 | 1 |

`net` depends on `script` too, so the server's dependency chain is `server` -> `net` + `script` -> `render` (**wgpu**) + `audio` (**kira + cpal**) + `mesh_edit` (**image, meshopt**).
- [V] at the manifest level: `render/Cargo.toml:19` lists wgpu, and `kira` in the lock depends on `cpal`.
- [I] So the headless `axcenity-server` compiles and links wgpu, cpal (and ALSA on Linux), image and meshopt that it never uses. That contradicts the intent in `docs/architecture/crates.md` ("never a window, GPU or audio output"): nothing calls them, but they are linked. I could not confirm with `cargo tree` because offline resolution failed.

---

## 3. Bottlenecks, couplings and hotspots

### 3.1 No class index, so many full-tree walks every frame (the biggest structural cost)
`core::World` has no per-class index (grep finds none [V]). `World::descendants` allocates a fresh `Vec` with an explicit stack (`core/src/world.rs:1194-1206`). Each subsystem finds "its" Instances by walking the whole Workspace (or the whole `game`) and filtering by class, every frame [V]:

| Where | Walks per frame | Citation |
|---|---|---|
| physics `finish_sync` (runs every frame, on the fast path too) | `joints::constraints` 1, `movers::read` 1, `vehicles::read` 2, plus `fracture::breakables` over every body | `physics/src/lib.rs:961-968`, `joints.rs:142-151`, `movers.rs:168-171`, `vehicles.rs:104-110` |
| audio | `sounds()` over `descendants(root)` (twice when no engine yet), plus `mix.rs` over root | `audio/src/lib.rs:174-178, 406-410, 682`; `mix.rs:257, 372` |
| anim | `Animators::update` | `anim/src/animators.rs:290` |
| character | `controllers()` | `character/src/lib.rs:746` |
| nav | `agents()` every frame. When agents exist, also `fingerprint()` (another walk that hashes every part's 6 properties) and `moving_parts()` (two more walks) | `nav/src/agents.rs:150, 408`; `nav/src/lib.rs:410-455` |
| procedural | `Generators::update` over `descendants(root)` | `procedural/src/lib.rs:68-73` |
| script step functions | `health.rs:291` (Workspace), `prompt.rs:213`, `humanoid.rs:674`, `tool.rs:389, 541`, `players.rs:181`, `sequence.rs:288` (root), `audio_settings.rs:58` (root) | 30 `descendants(` call sites in `script/src` outside tests |

- [I] That is roughly 15-20 O(N) walks per frame, each allocating a Vec, before any work is done.
- `docs/rendering/performance.md:217-240` shows this pattern has already caused slideshow-level problems (`Session::frame` 666 ms -> 2.0 ms), each fixed locally.
- A class index or tag index in `core`, keyed by `structure_version`, would remove the repeated walks across all of these crates at once.

### 3.2 World -> rapier sync: dirty-checking that still costs O(parts) per frame, with a full-rebuild cliff
- **Fast path** (`sync_changed_parts`, `physics/src/lib.rs:908-952`) [V]:
  - Runs when `structure_version` and the "held" set are unchanged.
  - It checks the revision of every watched WeldConstraint, Ragdoll and Inventory, then **iterates every body** comparing `Instance::revision`.
  - Physics' own `write_to_world` bumps the revision of every part that moved. So **every moving part is fully re-read (`PartState::read`) every frame**, only to find `state == part.seen`.
  - Non-primitive shapes (MeshParts, Terrain) are re-read every frame regardless (`lib.rs:925`).
- **Slow path** (`sync_from_world`, `lib.rs:782-905`) [V]:
  - Runs when any reparent happens **anywhere** in the World, because `structure_version` is global (`core/src/world.rs:813-816`; collision group edits too, `:428, :433`).
  - It walks all Workspace descendants, builds a `BTreeMap<InstanceId, PartState>` for every part, recomputes union-find assemblies and roots, and walks **the whole tree from `root`** to rebuild `watched` (`lib.rs:886-897`).
  - [I] So spawning one projectile, adding a UI element, or moving an item into ReplicatedStorage costs a full physics resync. The code comments call the full sync "most of a frame" with tens of thousands of parts (`lib.rs:906-907`).
- **Wake storm** [V]: `if anything_changed { self.rapier.wake_up_all(true) }` (`lib.rs:975`). Any script moving any part wakes every sleeping body in the world. The comment admits it is "cheap enough for v1 scene sizes".
- **Write-back** [V]: `write_to_world` runs **inside the substep loop** (`lib.rs:561`), so at 3 substeps a frame each moving part gets up to 4 `set_property` calls x 3. Each call bumps `revision` and pushes a `core::Change` onto the World's log (`world.rs:564-573`). It also converts quaternions to Euler degrees every step (`quat_to_orientation`, `lib.rs:1390-1393`), which loses precision and has gimbal issues; that conversion is forced by the `Orientation` property model.
- Strength [V]: `BTreeMap` ordering, fixed `STEP_SECONDS`, a tolerance on the accumulator, `MAX_STEPS_PER_FRAME`, and no scripts running between substeps together give reproducible stepping per frame rate.

### 3.3 The change log drives script events, and physics floods it
- Every physics write becomes a `RecordedChange` [V].
- `ScriptRuntime::fire_changes` (`script/src/lib.rs:2032-2042`) is called many times per frame: before and after each runtime entry point, and three times inside `step` (`lib.rs:1243-1301`). Each round hands **the full change list to both VMs** (`lib.rs:2044-2055`, `context.rs:615`).
- `changes::fire` (`changes.rs`) does at least 2 `BTreeMap` lookups (`Changed`, `PropertyChanged(name)`) per change, per side, plus `exists()`. That is cheap when nothing listens, but O(moving parts x 4 properties x substeps x 2 sides) per frame [I].
- **Quadratic cases** [V]:
  - `fire_descendant_added` scans all *later* changes for every ChildAdded (`changes.rs:91-104`): O(n^2) when a model with many parts is cloned in.
  - `fire_descendant_event` fires per ancestor x per descendant (`changes.rs:111-144`).
  - `fire_ancestry_changed` fires per descendant (`:146-166`).
  - Each of these allocates a `descendants` Vec.

### 3.4 How Lua reaches Instances (the per-access cost)
All [V], `script/src/instance.rs`:
- **A new userdata on every access.** `LuaInstance(id).into_lua(lua)` allocates a fresh userdata each time an Instance is returned (`instance.rs:150, 246`; `types.rs:954-958`). There is no interning or weak cache (grep finds none).
  - Equality works through `__eq` (`instance.rs:122-124`).
  - [I, likely bug] **Instances as table keys do not work**: `t[workspace.Part] = 1; print(t[workspace.Part])` gives nil because the two userdata are different objects, and `rawequal` fails. The common Roblox pattern `data[player]` would silently break. No test covers it. This needs a test, then interning (a weak-valued `id -> userdata` table).
- **A `String` allocated per property access.** `__index`/`__newindex` take `key: String` (`instance.rs:114-121`), so mlua copies the Lua string into a Rust `String` on every `part.Position`.
- **Lookups by string, in a long chain.** `get` (`instance.rs:136-249`) checks, in order:
  1. `humanoid::is_field` (a linear `contains` over two arrays)
  2. `StreamingState`, `StreamedIn`
  3. the `Name`, `ClassName`, `Parent`, `WaitForChild` match
  4. `AccessibleName`
  5. **the property `BTreeMap<&'static str, Value>`** (`core/src/world.rs:55, 114`): O(log P) string comparisons
  6. CFrame, then `attachment::computed`, `spline::computed`, `physical::computed`, `joints::current`
  7. Animator, Sequence and AnimationTrack special cases
  8. `class_member`, a **linear scan of the `ClassMethods` Vec** (`instance.rs:466-474`)
  9. events, then `find_first_child`

  So a method or child lookup pays every earlier check. `set` (`instance.rs:251-424`) has the same shape and borrows the World 2-4 times per write.
- **Value types are userdata.** `Vector3`, `CFrame` and `Color3` become userdata on every read (`types.rs:926-960`; `impl UserData for LuaVector3`, `types.rs:93`). Luau's native `vector` value type, which mlua supports with `luau`, is not used. [I] `part.Position + Vector3.new(...)` is therefore several heap allocations plus metamethod dispatch.
- **The whole World is moved per Lua call** [V]. `with_world` moves the World in and out with `std::mem::take(world)` (`lib.rs:2371-2378`). `World::default()` is `World::new()`, which **allocates a full DataModel with 13 service Instances and their default property maps** (`core/src/world.rs:300-340, 1245-1248`). That throwaway world is built on every `with_world` call: about 28 call sites in `context.rs`, run several times per frame per side. [I] This is pure waste, at most tens of µs per frame, but easy to remove (for example a cheap `World::placeholder()`, or lending `&mut World` through a scoped app-data guard).

### 3.5 Scheduler, coroutines and events
All [V] unless marked:
- **`take_due` touches every waiting task every step.** It drains and partitions the *whole* `waiting` Vec into two new Vecs each step, then sorts the due ones (`scheduler.rs:101-113`). That is O(W) allocations and moves per frame however few tasks are due. `task.cancel` and `forget_script` are linear `retain`s. [I] With thousands of `task.wait(5)` NPC loops, a binary heap keyed by `(due, order)` would make it O(due x log W).
- **A new coroutine per handler, per fire.** `fire_handlers` clones the handler list into a Vec, then calls `lua.create_thread(callback)` for each handler (`events.rs:211-245`). Heartbeat, Stepped and Touched handlers each allocate a Luau thread every frame. Roblox-style runtimes reuse threads for handlers that do not yield. `is_connected` is a linear scan per handler (`events.rs:166-170`).
- **Connection pruning every frame.** `Context::step` calls `Connections::forget` over *all* keys, plus `Callbacks::forget` (`context.rs:375-391`). That is O(connections) per frame.
- **Per-resume overhead.** `resume` runs `debugger::hold_while_paused`, `watchdog::enter/leave` and `debugger::enter/leave` (several `app_data` borrows) on every resume (`scheduler.rs:181-215`). The watchdog interrupt fires at every call and loop back-edge and reads the clock every 1000 interrupts (`watchdog.rs:44, 92, 205`). That is reasonable overhead.
- **Two VMs per process**, the server side and the client side in the same process even for single-player (`lib.rs:222-224`). Each frame steps both, fires changes on both, and copies messages through the `Loopback` transport.
- **Sandbox environments** are per-script tables with copied libraries (`sandbox.rs:220-261`). [I] Luau's "safeenv" fast paths (cached `GETIMPORT` builtins, fastcalls for `math.*`) need the default sandboxed globals or `Lua::sandbox(true)`. Custom environments loaded via `set_environment` generally lose them, so `math.floor` and similar calls in hot loops are slower than stock Luau. This should be benchmarked.
- **Recompiling clones.** Each started or cloned script recompiles its source (`context.rs:353-369`) and builds a new environment. [I] Spawning many NPC clones that carry Scripts pays a compile per clone. Compiled `Function`s could be cached by source hash.
- Determinism [V]: game-time clock, ordered ties, a 1 µs tolerance, and a 0.1 s frame cap (`docs/scripting/execution.md`). Scripts see a **variable dt**, while physics is fixed at 60 Hz.

### 3.6 `script` is a god-crate
- [V] It depends on 11 engine crates and holds most *gameplay*: humanoids, players and respawn, tools, prompts, health, abilities, dialogue, quests, sequences, spline followers, procedural, saves, levels, chat, and ownership. These are gameplay systems with Lua bindings, not scripting.
- `ScriptRuntime` has **about 40 fields** (`lib.rs:222-311`), and the host must call about 20 `step_*`, `fire_*` and `take_*` methods in a precise order (see `docs/architecture/frame-loop.md` and `player/src/main.rs:1440-1800`).
  - That ordering is duplicated in four hosts: player, editor `play.rs`, server, and `testing` `game.rs`. Nothing enforces it except comments.
- It depends on `render` for 3 plain-data types plus glam, and on `audio` for `SoundAction` and enum lists. Both should move to `core` or a small `*-types` crate. That would stop the server linking wgpu and cpal, and cut rebuild fan-out: any edit to `render` or `audio` recompiles `script`, `net`, `server`, `player`, `editor` and `testing`.
- Because `net` depends on `script` (rather than `script` exposing a message interface), the network layer reaches into `ScriptRuntime` through about 30 methods (`net/src/game.rs`, `scripts.*` calls at `:205-1184`).
- [I] A better split: `script` keeps the VM, sandbox, scheduler, events and the Instance binding. The gameplay systems (Humanoid, Players, Tools, Health, Abilities, Dialogue and the rest) move to a `gameplay` crate. Each one registers its Lua API through a trait and gets stepped by a schedule rather than by a hand-written host sequence.
- The `dev` profile does **not** optimize `script` (`Cargo.toml:157-184` lists only render, scene, physics, terrain, nav and mesh-edit). Editor Play runs the Lua glue unoptimized. Third-party `*` crates, including Luau, are at opt-level 3.

### 3.7 Replication diffing (net)
All [V] unless marked:
- **`Replicator::frame`** (`net/src/replicate.rs:411-500`):
  - Every frame it iterates every replicated Instance and skips those whose `revision` is unchanged. That is an O(N) scan per frame, cheap per item.
  - For any changed Instance it **clones the full `Sent` snapshot**: name `String`, every property `Value`, the attributes `BTreeMap` and the tags Vec (`replicate.rs:233-249`). It then diffs with a linear `find` per property, which is O(P^2) per Instance (`replicate.rs:517-528`).
  - Physics bumps the revision of **every moving part every frame**. So every moving part gets a full clone and diff each frame, even though its motion properties are filtered out and the diff is empty.
  - [I] This is O(moving parts x properties^2) of allocation per frame, on the server only and only while players are connected.
- When the tree changes (the global `structure_version`), `replicated(world)` is walked again, and `seen` (a `BTreeSet`) is rebuilt to find what was destroyed.
- **Serialization**: reliable updates, snapshots and RemoteEvent payloads are **JSON** (`handshake.rs:118-119`, `wire.rs:1-3`). [I] Snapshots of large worlds are many megabytes and slow to encode. Channel memory is 5 MiB (`transport.rs` `CHANNEL_MEMORY`), so a big snapshot can exceed it. Should be checked.
- **Motion** is well designed: quantized 32-byte records, priority by error x nearness, a byte budget per player, a reliable "settle" resend (`motion.rs:1-17`).
  - Cost: `MotionSender::frame` is O(replicated parts) **per peer**, with a `BTreeSet` built from all parts each frame (`motion.rs:257-290`). That is O(peers x N log N).
  - The client's `Smoother` writes `Position` and `Orientation` through `set_property` each frame, which feeds the client VM's change log again.
- **Security**: netcode runs with `ServerAuthentication::Unsecure` and `ClientAuthentication::Unsecure` (`transport.rs:240, 415`), so there are no connect tokens or encryption. The server's `CharacterCheck` limits movement cheating (strength), but nothing stops packet spoofing or reading traffic on a LAN or the internet. [I] That is acceptable for LAN; it should be documented as a limitation before any internet hosting.
- [I] **Determinism and authority**: the server is authoritative, with client-owned assemblies (characters, vehicles). There is no lockstep or rollback. rapier is not bit-identical across CPUs (`docs/architecture/physics.md`, the Phase 34 note), and `enhanced-determinism` is off. Cross-machine determinism is therefore not a goal, which is consistent with the design.

### 3.8 Navigation rebuild hitch
All [V] unless marked:
- When any NavigationAgent exists, `fingerprint()` hashes every Part, MeshPart, Seat and similar in the Workspace every frame (`nav/src/lib.rs:410-455`).
  - It formats non-trivial values with `format!("{other:?}")`.
  - It checks `skip.contains(&id)` against a `Vec`, which is O(parts x character parts).
- It does **not** exclude unanchored, physics-driven parts. So any falling crate changes the fingerprint, and `NavMesh::build` reruns **synchronously on the main thread** every `REBUILD_SECONDS = 0.5` (`agents.rs:36, 335-357`).
- The `Cargo.toml` comment puts a build at about 50 ms optimized for a 512-stud terrain (`Cargo.toml:176-178`). [I] That means periodic 50 ms frame hitches in a level with agents and any moving props.
- There are **two independent navmesh caches**: `nav::Agents` and `script::humanoid` (`humanoid.rs:638-655`). Both build the same mesh, so Humanoid MoveTo plus agents means double builds.
- Suggested fixes: build on a worker thread, use the anchored-only fingerprint, and share one mesh.

### 3.9 Audio and the other per-frame syncs
- [V] `GameAudio::sync` walks every Sound in the game every frame and re-reads its properties (`audio/src/lib.rs:668-685`). That is fine for tens of sounds, but O(instances) for the tree walk itself.
- [V] The Kira manager starts lazily. `actions.iter().any(...) || sounds(world).any(...)` walks the whole tree each frame until audio is wanted (`lib.rs:174-178`).

### 3.10 Single-threaded sections
- [V] The whole simulation frame runs on the main thread, in steps 0-6: network, character, nav, scripts (2 VMs), physics, audio sync, animators and procedural.
- The only worker threads are CSG (`script/src/csg.rs:153`) and asset texture loading. rapier runs without `parallel`. Animator posing for many characters and navmesh building are the obvious first candidates to move off the main thread [I].
- `docs/rendering/performance.md:226` already describes a case where "the work was on one thread (1 of 32)".

---

## 4. Strengths worth preserving
- **A deterministic, documented frame order** (`docs/architecture/frame-loop.md`), with step numbers echoed as comments in `player/src/main.rs`. Ordered containers (`BTreeMap`) are used wherever iteration order affects simulation (`physics/src/lib.rs:121-124`).
- **Fixed-step physics** with a tolerant accumulator, a step cap and no script code between substeps. Touch events are deferred and ordered (sorted pairs, 3-step hysteresis).
- **Generational-arena Instances, with Lua holding only IDs.** Stale handles fail cleanly ("this Instance has been destroyed"). There is no `Rc<RefCell>` graph and no Lua-to-Rust ownership cycles.
- **Deferred change events** with a round limit (`MAX_CHANGE_ROUNDS`), and handlers that only see changes made after they connected. This is a robust answer to re-entrancy.
- **A strong sandbox**: an allowlist environment per script, `lua.globals()` never exposed, source-only loading (no bytecode), and a watchdog that yields rather than errors, so `pcall` cannot swallow it. `unsafe` code is confined to `debugger.rs`.
- **The scheduler model**: game-time clock, ordered ties, per-script ownership of coroutines (which enables hot reload via `forget_script`), and a single `resume` choke point for the watchdog and debugger.
- **Revision counters and a structure version in `core`.** The dirty-tracking primitive already exists; physics and replication use it. It needs extending: a class index, and a per-subtree or per-service structure version.
- **The networking design**: snapshot plus diff, with Instance IDs stable across machines. Motion is quantized and budgeted. The server checks character movement. Streaming and network ownership exist. The loopback transport can simulate bad networks for tests.
- **Heavy test coverage** (about 25-35% of LOC in these crates is tests), and benchmarks with a recorded baseline (`player/src/bench_tests.rs`, `docs/rendering/performance.md`).
- **Pins and licences documented** in the workspace `Cargo.toml`. cargo-hakari keeps `-p` builds fast.

---

## 5. Top recommendations (for Phase 2 to weigh)
1. **Add a class/tag index to `core::World`**, maintained on create, reparent and destroy, plus a no-alloc descendants iterator. Replace the ~20 per-frame full-tree walks. This has the highest leverage across physics, audio, anim, nav, procedural and script.
2. **Make physics sync event-driven or scoped**:
   - Make `structure_version` per-Workspace, or keep a list of added and removed parts, so unrelated reparents do not trigger a full resync.
   - Do not count physics' own writes as "changes" (write a separate physics revision, or skip re-reading parts whose `seen` was just written).
   - Wake only the bodies near a change instead of calling `wake_up_all`.
   - Move `write_to_world` out of the substep loop, so it runs once per frame.
3. **Interning and allocation in the Lua binding**:
   - Intern `LuaInstance` userdata (also fixes Instances as table keys; write the test first).
   - Take `mlua::String` or `BorrowedStr` keys instead of `String`.
   - Dispatch members through a per-class perfect-hash or pre-resolved table.
   - Consider Luau native `vector` for Vector3.
   - Stop building a throwaway `World::new()` in `with_world`.
4. **Scheduler and events**: use a binary heap for waits, reuse threads for handlers, and prune connections on Destroy rather than every frame.
5. **Split up the `script` god-crate**:
   - Move plain data types (`SoundAction`, `GraphicsSettings`, `Capabilities`, `Camera`) into `core` or `*-types` crates, so `server` stops linking wgpu and kira.
   - Extract the gameplay systems behind a registration and scheduling trait, so the host frame order lives in one place instead of four.
6. **Replication**: diff only Instances whose non-motion revision changed (keep a separate motion revision). Use a binary encoding (bincode or postcard) for updates and snapshots. Turn on netcode secure tokens before any internet play.
7. **Nav**: fingerprint anchored geometry only, build the navmesh on a worker thread, and share one cache between Agents and Humanoids.
