# Phase 1 audit, part C: main loops and tooling

Scope: `player`, `editor`, `project`, `ui`, `blocks` / `blocks-runtime`, `plugins`, `testing`, `server`, `workspace-hack`, plus build and CI (`.github/workflows`, `docs/development.md`, root `Cargo.toml` profiles, `packaging/`).
Repo: `/home/user/axcenity-engine` (read only). All paths below are relative to it.
Labels: **[V]** means I checked it in the source (file:line given). **[I]** means an inference that I have not measured.

---

## 0. Headline findings

1. **[V] No shared game-loop or schedule abstraction.** Four hosts each order the frame steps by hand:
   - player: `crates/player/src/main.rs:1424-1825`
   - editor Play: `crates/editor/src/play.rs:507-760`, plus `viewport.rs:1257-1362` for particles, streaming, UI and draw
   - server: `crates/server/src/main.rs:175-208`
   - tests: `crates/testing/src/game.rs:129-230`

   The only place that defines the order is the prose in `docs/architecture/frame-loop.md`. The hosts have already drifted apart (see §1.4).
2. **[V] The in-game overlay UI is assembled twice.** Hotbar, ability bar, quest tracker, dialogue box, leaderboard, chat, prompts, console and particles are added to the draw list by near-identical code in `player/src/main.rs:1698-1789` and `editor/src/viewport.rs:1288-1356`.
3. **[V] The editor is a single 162k-LOC crate.** `app/` alone is 47.6k LOC, and `EditorApp` has 109 fields (`app.rs:181`). The crate depends on every engine crate.
4. **[V] Graphics versions are locked together.** The editor's viewport renders with `axcenity-render` on eframe's own wgpu `Device`. So eframe 0.32.3, egui_dock 0.17, egui-phosphor 0.10, wgpu 25.0.2 and winit 0.30.13 can only be upgraded together (`Cargo.toml` comments at eframe, egui_dock and egui-phosphor).
5. **[V] Project data is a monolithic JSON world file plus loose source files.** One `world.json` per level, written by a hand-rolled deterministic serializer. Scripts and CSS are separate files. There are three version axes: manifest `FORMAT_VERSION = 4`, world `format_version = 1`, and the `.axgame` pack `VERSION = 1`.
6. **[V] Hot reload uses a portable polling watcher.** It wakes every 500 ms, walks and stats all watched paths, and waits a 100 ms settle (`project/src/watch.rs:24-27, 168-187`). The editor can run up to three of them: project, engine sources, and plugins.
7. **[V] CI is reasonable in shape but has gaps:**
   - no performance or benchmark gating (benchmarks are `#[ignore]`)
   - GPU tests quietly skip when there is no adapter
   - the toolchain is unpinned `stable` while CI builds with `-D warnings`
   - there is no release-profile tuning
   - the dedicated server and the test runner are not shipped in releases
   - `docs/development.md` says "CI is off", which contradicts `ci.yml`

---

## 1. The main loop

### 1.1 Player frame (`crates/player/src/main.rs`)

**Event loop.** It uses winit 0.30 `ApplicationHandler`. `about_to_wait` calls `request_redraw()` unconditionally (`main.rs:1848-1852`), so the game runs flat out. Pacing comes from the surface's present mode. The renderer uses `surface.get_default_config` (`render/src/renderer.rs:255`), which is normally Fifo (vsync) **[I]**. All work happens on the main thread inside `WindowEvent::RedrawRequested` (`main.rs:1424`).

**Timing.** `frame_seconds` caps dt at `MAX_FRAME_SECONDS = 0.1` (`main.rs:58, 1171-1176`). It is variable-step everywhere, except that physics uses an internal fixed-60 Hz accumulator (`docs/architecture/frame-loop.md` step 3).

One frame, in order:

| # | Step | Code (main.rs) |
|---|---|---|
| pre | dt, console timescale, Stopwatch | 1426-1428 |
| pre | UI mouse clicks: a **full extra UI layout** per press/release change (`layout_ui_interactive`) | 1434 → 901-925 |
| pre | UI keyboard/gamepad focus navigation | 1435 |
| pre | UI events → scripts (MouseEnter/Leave, button sounds) | 1438-1440 |
| pre | `--watch` asset hot reload (`watch::reload`) | 1444-1451 |
| 0 | **Net receive** `network.receive`; Kick check; switch to follower mode | 1453-1477 |
| 1 | Controller input → `characters.step` → `fire_seated` / `fire_landed` → `step_humanoids` → `step_players` → `agents.step` → `fire_sensed` | 1481-1521 |
| 1 | `begin_camera`, cursor, `input_events` → `step_prompts` → `step_tools` → `step_abilities` | 1522-1536 |
| 2 | `scripts.step` (tweens, coroutines, RemoteEvents, splines, sequences) → `stepped` | 1540-1546 |
| 3 | held parts, far-sleep, kinematic characters and agents, `apply_shatters`, **`physics.step`** (accumulator) | 1550-1570 |
| 4 | `fire_touched` → `fire_touch_ended` → `fire_breaks` → `step_health` → `heartbeat` | 1572-1589 |
| 4 | **Audio** `apply_audio` (kira) | 1591-1599 |
| 4 | Particles (CPU or GPU compute) | 1602-1611 |
| 4 | **Animators** `update` → Animator events → `place_tools` | 1614-1629 |
| 5 | Level loads, loading screen `advance` | 1632-1641 |
| 6 | `render_step` (BindToRenderStep, RenderStepped) → `end_camera` (`step_camera`) → graphics settings change | 1648-1662 |
| 6 | Streaming step; **draw list** `scene::build_draw_list_with_ui_interaction` (includes taffy UI layout) | 1663-1688 |
| 6 | Overlay UI items: hotbar, abilities, quests, dialogue, leaderboard, chat, prompts, particles, console, splash | 1698-1786 |
| 6 | **Net send** `network.send` | 1796 |
| 6 | **Render** `renderer.render(&draw_list, pre_present_notify)` (synchronous); shader errors | 1803-1823 |

`print_output(&mut self.scripts)` is called about 10 times per frame as a manual flush.

**[V]** Setup is also hand-written: arguments, `.axgame` unpack to a temp folder, project load, levels, scripts and animators (`main.rs:71-266`).

### 1.2 Editor Play frame

**Driver.** The driver is `eframe::App::update` (`editor/src/app.rs:980`). It calls `hot_patch::call(patched, || self.frame(...))` (`app.rs:983`), then `pace_frame()` (`app.rs:1132-1143`).

Pacing sleeps and then **spin-waits up to 2 ms on the UI thread** to hold a fixed refresh grid (`wait_until`, `app.rs:1079-1089`). It is active while frames are "steady", which `keep_frames_steady` decides at `app.rs:1103`.

Inside `EditorApp::frame` (`app.rs:1148`):
- **Pre-steps:** engine-source watcher and rebuild (`engine_frame`, 1170), project loading, background texture loads, `project.watcher.wake` and `check_disk` (1185-1188), plugins' frame (1193), probe bake, terrain, `generate_frame` (SplineMesh), palette, shortcuts.
- **Play step:** runs only when `playing() && !paused()` (`app.rs:1222-1306`). It computes its own dt from `Instant` (cap `MAX_FRAME_SECONDS`), clones egui events, handles dev console, chat and text-box editing, and does UI navigation (`navigate_play_ui`). It then runs `game_keys.read`, applies the console timescale, calls **`play_frame`** (`app/session.rs:787`), and finally `request_repaint()`.

`play_frame` (`app/session.rs:787-880`):
1. Feeds pending UI events, button activations and focus-lost to the session.
2. Sets the viewport size and camera.
3. Computes motion from the input map.
4. Calls **`session.frame`** under `hot_patch::call`.
5. Records profiler, network, mixer and particle emits.
6. Handles Kick and pause.

`Session::frame` (`editor/src/play.rs:507-760`) runs its body inside `guard` = `catch_unwind` (`play.rs:1093`):

| # | Step | play.rs |
|---|---|---|
| 0 | Network requests answered offline (Host/Join fail with `PLAY_IS_OFFLINE`), then **`network.receive`** over the in-memory loopback (multi-client Play, Phase 64) | 529-549 |
| 1 | `characters.step` → seats → landings → `step_humanoids` → `step_players` → `agents.step` → sensed | 562-605 |
| 1 | `set_camera`, `input_events`, `step_prompts`, `step_tools`, `step_abilities` (pause check) | 617-633 |
| 2 | `scripts.step` → `stepped` (pause checks) | 635-646 |
| 3 | held parts, far-sleep (inline copy of the player's `set_far_physics_sleep`), kinematic, `apply_shatters`, `physics.step` | 650-669 |
| 4 | touches, touch ends, breaks, `step_health`, `heartbeat` | 671-690 |
| 5 | level loads → loading screen | 692-707 |
| 6a | **`render_step` BEFORE animators** (the player runs it after them) | 711-714 |
| 4b | audio (only device 0 is heard), **animators**, `place_tools` | 734-751 |
| 6b | `step_camera`, **`network.send`** | 754-758 |

**Drawing happens later**, when egui draws the Scene tab (`editor/src/viewport.rs`):
1. Game UI is laid out **twice**: `base_layout` for hit-testing (`viewport.rs:1052`), then `self.ui.layout(...)` (`viewport.rs:~1087`).
2. Particles are emitted and updated **in the viewport, not the session** (`viewport.rs:1257-1267`).
3. Streaming steps in the viewport (`viewport.rs:1270-1276`).
4. `build_draw_list_posed` (`viewport.rs:1288`), then the overlay UI items (1299-1355).
5. `target.renderer.render(&draw_list)` into an **offscreen texture** registered with egui-wgpu (1362).
6. The texture is painted as an `egui::Image`, followed by selection and overlay painting (1371+).

**[I]** Because particle simulation lives in viewport drawing, it only advances while a Scene viewport is drawn.

### 1.3 Server and test hosts

- **Server** (`crates/server/src/main.rs:81-208`, 372 LOC):
  - Loop: `loop { frame(); sleep(FRAME - elapsed) }` with `FRAME = 16_667 µs` (`main.rs:29, 150-167`). dt is real time, capped at 0.1 s.
  - Frame: `receive` → `step_players` → `step` → `stepped` → `apply_shatters` → held, far-sleep → `physics.step` → touches, breaks → `step_health` → `heartbeat` → `send`.
  - No characters, agents, animators or prompts. Characters are moved by clients (Phase 66).
  - Single-threaded. There is no graceful shutdown path **[V: infinite loop, no signal handling]**.
- **Tests** (`crates/testing/src/game.rs:129-230`): `TestGame::frame` uses a **fixed** `FRAME_SECONDS = 1/60`. It runs steps 1-4 plus animators, `place_tools`, render_step and camera, and drops sound actions and particle emits. There is no network step.

### 1.4 Shared vs duplicated

**Shared** **[V]**: every step is a method on the shared subsystems (`ScriptRuntime::*`, `Controllers`, `Agents`, `Physics`, `Animators`, `Network`, `GameAudio`). What is duplicated is the orchestration.

**Duplicated, with drift** **[V]**:

| Aspect | player | editor Play | server | testing |
|---|---|---|---|---|
| Owner struct | `App`, 53 fields (`main.rs:559`) | `Session`, 19 fields + `EditorApp` | locals in `main` | `TestGame` |
| Net receive/send | yes | yes, loopback (**`frame-loop.md` says "editor Play has no network step"**, which is stale) | yes | no |
| dt | variable, cap 0.1 | variable, cap; egui-driven | variable, cap 0.1, sleep to 60 Hz | fixed 1/60 |
| render_step vs Animators | Animators → render_step | render_step → Animators | n/a | Animators → render_step (`game.rs:207-224`) |
| Audio vs render_step | audio before | audio after | none | dropped |
| Particles | in frame (`main.rs:1602`) | in viewport draw | none | dropped |
| Streaming step | in frame (`main.rs:1666`) | in viewport (`viewport.rs:1271`) | none | none |
| Panic isolation | process panic hook (`logs.rs`) | `catch_unwind` per frame | none | none in crate (no `catch_unwind`; nextest process isolation) |
| Overlay UI assembly | `main.rs:1698-1789` | `viewport.rs:1299-1355` | n/a | n/a |
| Startup (levels, starter character, scripts, loading GUI) | `main.rs:100-266` | `play.rs:124-215` | `server/main.rs:100-145` | `game.rs` setup |

**[I]** Every new phase that adds a step ("Since Phase NN, step X also...") has to be applied to four hosts by hand. `frame-loop.md` already carries 10 "Since Phase N" addenda. A shared `GameLoop` or stage list, with host hooks for input, draw and net, would remove most of this.

---

## 2. Subsystem map

### 2.1 LOC (wc -l over *.rs, tests included)

| Crate | LOC | Files | Tests (`#[test]`) | Key types and entry points |
|---|---|---|---|---|
| `editor` | 162,116 | 320 | 902 (~32k LOC in `*test*.rs`) | `EditorApp` (`app.rs:181`, 109 fields), `Session`/`Play` (`play.rs`, `play/multi.rs`), `Viewport` |
| `player` | 15,831 | 40 | 152 (14 ignored benchmarks) | `App` (`main.rs:559`), `watch::Arguments`, `dev_console`, `logs` |
| `ui` | 15,195 | 29 | 102 | `layout_ui*` (`lib.rs:345-376`), `UiInteraction`, CSS `style.rs` (`resolve_styles` :302), `Gamepads` |
| `project` | 15,059 | 34 | 151 | `load` (`lib.rs:423`), `save_world` (:641), `Watcher` (`watch.rs`), `migrate::upgrade`, `pack_game` (`packed.rs`), `versions`, `validate`/`check_assets` |
| `blocks` | 8,120 | 6 | 45 | `from_lua` (2,663) + its own Luau reader `lua.rs` (1,403) |
| `blocks-runtime` | 6,789 | 8 | 19 | `BlockScript`, `translate` (→ Lua), `node_graph`, `ui_graph`, `behavior_tree`, `dialogue` |
| `testing` | 1,653 | 7 | 20 | `Runner`, `TestGame` (`game.rs:27`), `junit`, bin `axcenity-test` |
| `plugins` | 1,533 | 5 | 7 | `Plugin`, `Toolbar`, `DockWidget`, `Request`, `find_plugins` |
| `server` | 372 | 2 | 3 (+`tests/headless.rs`) | `main`, `frame` |
| `workspace-hack` | 3 | 2 | 0 | generated by hakari |

**Editor by top-level module** (parent file + folder):

| Module | LOC | Of which tests |
|---|---|---|
| `app` | 47,580 | 19,764 |
| `animation` | 16,383 | |
| `model_studio` | 12,535 | |
| `ui_designer` | 10,823 | |
| `block_editor` | 9,012 | |
| `viewport` | 5,935 | |
| `start` | 5,247 | |
| `asset_browser` | 3,010 | |
| `terrain_studio` | 2,784 | |
| `import_dialog` | 2,572 | |
| `node_graph` | 2,441 | |
| `play` | 2,435 | |
| `gizmo` | 2,384 | |
| `properties` | 2,340 | |
| `material_editor` | 2,187 | |
| `history` | 1,926 | |
| `material_graph` | 1,874 | |
| `layout` | 1,571 | |
| `tree` | 1,503 | |
| `ui_handles` | 1,418 | |

Then `sequencer`, `explorer`, `theme`, `build_tools`, `data_table`, `profiler` (about 1k each), and about 40 smaller modules.

The architecture is a single god-object `EditorApp`, with behaviour spread over about 80 `impl EditorApp` child files in `app/` (`use super::*`). There is no internal crate boundary: panels reach `EditorApp` fields directly.

### 2.2 Third-party dependency and provider matrix (Cargo.lock versions)

| Crate (locked) | Used by | How it hooks in |
|---|---|---|
| eframe / egui / egui-wgpu / egui-winit **0.32.3** | editor | `eframe::run_native` with `Renderer::Wgpu` and custom `WgpuConfiguration` adding TIMESTAMP_QUERY (`editor/src/main.rs:107-170`). `persistence` stores layout and settings (`app.rs:994-1050`). `accesskit` 0.19. No glow. |
| egui_dock **0.17.0** | editor | dock tabs (`layout.rs`, `app/chrome/panes.rs`), serde layout |
| egui-phosphor 0.10 | editor | icon font |
| wgpu **25.0.2** (single copy in lock) | render, editor (via `axcenity_render::wgpu`) | Editor viewport renders into a texture registered with `egui_wgpu::Renderer` (`viewport.rs:1240-1254`), so it must be the same wgpu major as egui-wgpu |
| winit **0.30.13** | player (`ApplicationHandler`), editor via eframe | player: `main.rs:1241-1852` |
| taffy **0.7.7** | ui | a new `TaffyTree` per container per frame (`ui/src/lib.rs:1164`); no caching |
| glyphon 0.9 / cosmic-text 0.14 | render | text shaping and atlas; `ui` only emits text draw items |
| gilrs **0.11.2** | ui (`gamepad.rs`), player and editor Play | controller focus navigation (pulls `inotify` on Linux) |
| rfd **0.17.2** | editor | **synchronous** `rfd::FileDialog` on the UI thread (22 call sites, e.g. `app/sky.rs:28`) |
| ureq **3.5** (rustls) + zip **2.6.1** (deflate) | editor `ambient_cg.rs`; zip also in `project` (backup) | `std::thread::spawn` workers (`ambient_cg.rs:348, 386, 518`); hakari `final-excludes` keeps http out of the player |
| subsecond / dioxus-devtools / dioxus-cli-config **0.7.10** | editor, `hot-patch` feature only | `hot_patch::call` wraps `EditorApp::frame` (`app.rs:983`) and `Session::frame` (`app/session.rs:841`) |
| notify | **not used** | polling `project::Watcher` instead (`project/src/watch.rs`) |
| mlua 0.12.2 + `luau0-src = "=0.22.0"` exact pin | script, plugins, blocks (tests) | the pin exists because of a bytecode-version mismatch (`Cargo.toml` comment) |
| kira 0.12.5, rapier3d 0.36.1, renet 2.0 | audio, physics, net | other auditors' scope |
| image 0.25, png 0.18, memory-stats, regex, env_logger, pollster | various | |

**Duplicate versions in the lock [V]:**
- **glam ×5:** 0.30, 0.31, 0.32, 0.33, 0.34. The engine is on 0.34. rapier's `glamx`, kira and workspace-hack are on 0.33. nalgebra's convert features pull 0.30-0.33.
- windows ×3, hashbrown ×3, getrandom ×3, thiserror 1/2, syn 2/3, smithay-client-toolkit ×2, calloop ×2.

**[I]** These add compile time and need math conversions at the physics and audio boundaries.

---

## 3. Serialization and asset formats

### 3.1 Project folder on disk [V] (`project/src/lib.rs:1-20`, `examples/hello-part/`)

```
axcenity.json           manifest {name, format_version: 4, ...}          (FORMAT_VERSION lib.rs:143)
world.json              ALL Instances of the start level, one JSON tree   (world_file.rs)
levels/<name>.world.json  other levels, same format
scripts/**.server|client|module.lua   one file per script (plus block-script JSON)
ui/<ScreenGui>.uigraph.json           UI Event Graphs
*.css                   stylesheets anywhere
strings.json, input.json, collision-groups.json   optional
assets/models/*.fbx|glb, assets/terrain/<T>/{heights.png, paint-*.png, terrain.json},
assets/animations/*.anim.json, materials, data tables, prefabs...
.axcenity/              per-machine cache: history/, previous/ (migration backups), backup.json
```

- **World file** (`world_file.rs:1-67`):
  - Nested `{class, name, properties, attributes, tags, children}`.
  - Property types are implied by the class schema (`[0, 1, 0]` is a Vector3 or a Color3 depending on the property). A CFrame is `[x, y, z, qx, qy, qz, qw]`.
  - Defaults are omitted. Prefab links and overrides are inline.
  - Written by a **hand-rolled string serializer** with sorted keys (`write_world`, `world_file.rs:769`), so the text is deterministic and diffs cleanly. Read through `serde_json::Value`, then mapped against the schema (`parse_world`, :291).
  - Instance identity is by name path. ObjectValue refs are name paths, and "a path through siblings with the same name finds the first" (doc header).
- **It is one file per level, not one per instance.** Scripts are external files that get attached to Script Instances at load (`add_script_instance`, `scripts.rs:207`).
- **Saves are atomic per file:** write to `<file>.saving`, then `rename` (`lib.rs:669-676`). Every editor Save also records a version under `.axcenity/history` (`editor/src/app/files.rs:253`, `project/src/versions.rs`).
- **Exported game:** custom binary `.axgame` = `"AXCGAME\0"` + version + count + entries (`packed.rs:11-13, 45+`).
  - **Uncompressed**, built fully in memory as `Vec<u8>`.
  - Block scripts are translated to Lua at pack time.
  - Assets ship **raw** (FBX, PNG and so on, with no cooking).
  - The player unpacks it to a temp folder at start (`player/src/main.rs:95-103`), and so does the server (`server/main.rs:88-96`).

### 3.2 Versioning and migration [V]

- Manifest version 1 → 4 through `migrate::step` (`migrate.rs:71-78`). Each step is a whole-folder function.
- Before migrating, the whole project is copied into `.axcenity/previous/<v>/` (`migrate.rs:127`).
- Newer projects are refused with a clear message (`lib.rs:356, 489`).
- Separate in-file versions: world `format_version` 1 (`world_file.rs:84`, newer is refused), `.axgame` `VERSION` 1, plus content migrations (`migrate_animation.rs`, `migrate_physics.rs`).
- **[I]** Because world JSON omits defaults and resolves properties by name, adding a property needs no migration, but renaming or retyping one does. There is no per-property schema-version table.

### 3.3 Hot reload [V]

**Watcher** (`project/src/watch.rs`):
- A background thread wakes every **500 ms** and rebuilds the watched path list each time:
  - `Everything` = `world.json` + `find_scripts` + `find_stylesheets` + a recursive walk of `assets/`
  - `Assets` = `assets/` only
- It `stat`s every file. If anything changed, it sleeps 100 ms, re-stats, and filters files that are still being written.
- It sends the paths on an mpsc channel and calls a waker (egui `request_repaint`).
- The host diffs file contents against what it holds, so its own saves are no-ops.

**Editor reactions** (`app/files/watch.rs`):
- `world.json` changed: re-parse and `replace_world`, refused if there are unsaved edits (126-200).
- Script changed: tab sync. During Play, `Session::reload_script` runs the new chunk in place and keeps the last good version on a syntax error (`play.rs:847-872`).
- Stylesheet, asset or block-script changes are also handled.

**Player:** `--watch` covers assets only, with no script reload (`player/src/main.rs:1444-1451`, `watch.rs`).

**Engine code:**
- The engine-source watcher (`rebuild.rs:82`, `start_listing`) triggers a background `cargo build` and relaunches with a session handoff (`relaunch.rs`).
- With `dx serve --hot-patch`, changed function bodies are patched in through subsecond.
- This is debug builds from the source tree only (`app/engine.rs:1-60`).

---

## 4. Bottlenecks and friction

### 4.1 Frame orchestration (high)
- **[V]** Four hand-ordered loops plus two copies of overlay assembly (see §1.4).
- **[V]** The editor's step order differs from the player's in three places: render_step vs animators, audio placement, and particles and streaming being in the viewport.
- **[V]** `frame-loop.md`'s claim that editor Play has no network step is stale (`play.rs:547`).
- **[V]** `crates.md` lists no `net` dependency for the editor, but `editor/Cargo.toml` has `axcenity-net`.
- **[I]** This is the largest correctness risk ("works in Play, breaks in the player"). It also makes it hard to add a schedule-level feature such as fixed-step scripts, rollback, or a job graph for parallel stages.
- **[V]** Everything is single-threaded on the main thread, apart from rayon inside `render` (the only rayon user) and asset and texture workers. There is no stage parallelism.

### 4.2 Editor size and compile times (high for team scaling)
- **[V]** 162k LOC in one crate, 320 files. `EditorApp` has 109 fields and about 80 `impl` files.
- **[V]** The editor depends on all 20 engine crates plus `testing`.
- **[V]** `docs/development.md`: editor incremental rebuild about 5-6 s, cold build about 6 min on a 6-core laptop, about 550 MB debug editor binary.
- **[I]** Each editor edit recompiles the whole 162k-LOC crate's codegen units (incremental only partially helps). Splitting panels (animation, model_studio, ui_designer, block_editor: about 50k LOC combined) into sub-crates behind a narrow `EditorContext` trait would cut rebuilds and enforce boundaries.
- **[V]** The editor's `CLAUDE.md` is meant to be under 10 lines but is about 10 very long lines that encode most of the module map. Docs are carrying structure that the code does not enforce.

### 4.3 egui immediate-mode costs (medium)
- **[V]** During Play, `request_repaint()` runs every frame (`app.rs:1305`), so the whole editor UI (dock, panels, Explorer, Properties) is rebuilt each game frame.
- **[V]** The Explorer rebuilds a `Vec<Row>` with a `String` clone per visible-or-expanded node (`explorer.rs:286-311`) and lays out all of them. There is no `ScrollArea::show_rows` virtualization anywhere in the editor (grep found none).
- **[V]** Game UI in the viewport is laid out twice per frame (`viewport.rs:1052` and ~1087). Each layout re-runs the CSS cascade for every UI instance and builds new taffy trees (`ui/src/lib.rs:1160-1164`, no cache).
- **[V]** The frame pacer spin-waits up to 2 ms per frame on the UI thread (`app.rs:1079-1089`). Its purpose, VRR flicker avoidance, is documented.
- **[V]** Synchronous `rfd` dialogs block the UI and Play loop while open.
- **[I]** Editor Play frame time = game frame + full egui rebuild + paint, so Play performance is not representative of the player. The Profiler records `Session::cost` separately, which helps.

### 4.4 UI crate (medium)
- **[V]** There is no retained style or layout cache, no dirty tracking, and a new `TaffyTree` per container.
- **[V]** The player does an extra full layout on every mouse press or release (`main.rs:901-925`).
- **[I]** UI cost is O(UI instances × stylesheet rules) every frame. That is fine for HUDs and risky for inventory-heavy games.

### 4.5 Polling watcher cost (low-medium)
- **[V]** Every 500 ms it does a full recursive walk plus stat of `assets/`, scripts and CSS. The editor can run three watchers (project, engine sources when rebuild or hot-patch is on, plugins: `app/plugins.rs:95`).
- **[I]** This is cheap for small projects. With tens of thousands of asset files, or the engine source tree on a slow or networked filesystem, it is a constant background I/O cost, and latency is 0.5-0.6 s.
- The choice is documented as intentional (portability, no extra library).
- **[I]** `notify` (with a polling fallback) would cut both. gilrs already pulls in `inotify` on Linux.

### 4.6 Version lock: eframe / wgpu / winit (high, strategic)
- **[V]** The editor shares eframe's `wgpu::Device` with `axcenity-render` (`viewport.rs:1240-1254`, `main.rs:107-121`). So render's wgpu major must equal egui-wgpu's.
- **[V]** The root `Cargo.toml` comments say eframe 0.32, egui_dock 0.17 and egui-phosphor 0.10 must be upgraded together with wgpu 25 and winit 0.30.
- **[I]** Newer egui releases target newer wgpu majors (for example egui 0.33 → wgpu 27). Each renderer upgrade is therefore blocked on egui_dock and egui-phosphor releasing in step, and vice versa. Renderer features from newer wgpu, such as mesh shaders or newer ray-query APIs, wait on the editor's UI stack.
- Possible decoupling **[I]**: render the viewport in a separate wgpu device or surface (a child window or shared texture), at a cost in complexity.

### 4.7 Extensibility: plugin API surface (medium)
- **[V]** The plugin API is Lua only (`plugins/src/api.rs`):
  - `plugin:CreateToolbar` / `CreateButton`, `CreateDockWidget` (whose content is a **game ScreenGui**, `lib.rs:67-71`)
  - `GetSetting` / `SetSetting`, `Read` / `WriteProjectFile` (project-only), `GetMouse`, `Activate`
  - `Selection`, `ChangeHistoryService` (`SetWaypoint` / `Undo` / `Redo`)
  - Requests: `Select`, `Waypoint`, `Undo`, `Redo` only (`lib.rs:74-81`)
- **[V]** Plugins cannot do the following:
  - custom property editors
  - importers or exporters
  - viewport drawing or gizmos
  - new asset types or menu items
  - running Play or tests
  - native (Rust) extensions
- **[V]** There is no plugin ABI. Engine extension means editing the monolithic editor crate. The `testing` crate is reusable (Tests panel and CLI share `Runner`), which is good.

### 4.8 Asset and pack pipeline (medium)
- **[V]** `.axgame` is uncompressed, built in memory, raw source assets with no cooking, and unpacked to temp at every launch.
- **[I]** For large games this costs startup time and disk churn. FBX is parsed at runtime by ufbx in shipped games.

### 4.9 Build, CI and release (medium)
- **[V]** CI (`.github/workflows/ci.yml`) is good:
  - fmt
  - hakari drift check
  - 3-OS build with `--locked`
  - **player-has-no-editor-code `cargo tree` gate**
  - clippy `-D warnings`, also with `--features hot-patch`
  - nextest `--profile ci`
  - doc tests
- **[V]** Gaps:
  - `rust-toolchain.toml` is `channel = "stable"` (unpinned) and `RUSTFLAGS: -D warnings` is set, so a new stable clippy or rustc lint can break CI with no code change.
  - Benchmarks (`player/src/bench_tests.rs:235, 288`; `editor/src/play/tests.rs:614`) are `#[ignore]`, so there is no performance regression tracking.
  - GPU tests print "skipped: no graphics adapter" and pass (e.g. `render/src/renderer/tests/ambient_occlusion.rs:14`). CI installs no software Vulkan (lavapipe) on Linux, so **[I]** render and player GPU tests probably do not run there.
  - Editor snapshots are written only when `AXCENITY_SNAPSHOTS` is set. They are not compared against golden images (`editor/src/snapshot.rs:1`).
  - No `axcenity-test examples/...` game-test run and no server headless check in CI. Those are only in `scripts/check-platform.sh`.
- **[V]** `docs/development.md` says "CI is off, so check Windows, macOS and Linux on those computers". This contradicts `ci.yml` (push/PR triggers). Either the workflow is disabled in the repo settings or the doc is stale.
- **[V]** There is no `[profile.release]` (no LTO, no `codegen-units = 1`, no `panic`, no `strip`). Only dev-profile per-package `opt-level = 3` for render, scene, physics, terrain, nav and mesh-edit, plus `"*"`.
- **[V]** `release.yml` builds only `axcenity-editor` and `axcenity-player` (with macOS universal binaries through `lipo`). `axcenity-server` and `axcenity-test` are not shipped, and grep found no code signing or notarization steps.
- **[V]** workspace-hack (hakari) is well used: the doc measures `-p axcenity-player` builds dropping from over 5 min to 5 s, and target size from 23 GB to 9 GB.

### 4.10 Blocks (low)
- **[V]** `blocks` has its own hand-written Luau reader (`blocks/src/lua.rs`, 1,403 LOC) for code → blocks, separate from the real Luau parser that mlua embeds.
- **[I]** Its grammar coverage can drift from Luau's. Round-tripping is tested (`blocks/src/tests.rs` is 3,278 LOC).

---

## 5. Strengths worth keeping

- **[V]** A written, numbered frame order with per-step comments in every host. It is easy to audit, even though it is duplicated.
- **[V]** Strict layering enforced in CI: the player has no egui, blocks or plugins (`ci.yml` cargo-tree gate). `render` is Instance-free.
- **[V]** Editor Play is isolated: a cloned world, `catch_unwind` per frame (`play.rs:1093`), breakpoints and pause, and multi-client Play over the in-memory loopback using the same `net` code as real networking.
- **[V]** Deterministic, human-readable, diff-friendly world JSON. Scripts and CSS are plain files editable in VS Code. Saves are atomic. There are versioned migrations with automatic backups, and version history and checkpoints.
- **[V]** A damaged file never blocks a project load (`Project::problems`).
- **[V]** Headless hosts already exist (`server`, `testing::TestGame`). They make good seeds for a shared, renderer-free simulation core.
- **[V]** Good build-speed hygiene: hakari, dev-profile opt-levels, `line-tables-only` debug info for dependencies, nextest GPU test group, and a documented rebuild, relaunch and hot-patch workflow.
- **[V]** Plugin dock widgets reuse the game UI system (ScreenGui), so there is one UI stack to maintain for plugin panels.

---

## 6. Suggested follow-ups for Phase 2 (inferences)

1. Extract a `GameSim` or `Schedule` in a new crate (or in `script`). It would own the world, scripts, physics, controllers, agents, animators, particles and streaming, and expose ordered stages with host hooks. player, editor Play, server and testing would become thin hosts. Move overlay UI assembly into `ui` or `scene` as one function.
2. Split the editor into sub-crates by panel family, behind an `EditorContext` trait, starting with `animation`, `model_studio`, `ui_designer` and `block_editor`. Add row virtualization to the Explorer and Properties.
3. Cache UI style and layout with dirty flags, and lay out once per frame in the editor viewport.
4. Plan a decoupling or upgrade path for egui and wgpu, and track upstream egui_dock releases.
5. CI improvements:
   - pin the toolchain
   - add lavapipe on Linux so GPU tests really run
   - add a scheduled benchmark job
   - run `axcenity-test` and the server headless check
   - add `[profile.release]` tuning
   - ship `axcenity-server`
6. Consider `notify` with a polling fallback, and asset cooking plus compression in `.axgame`.
