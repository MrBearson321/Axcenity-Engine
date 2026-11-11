# Engine capability map for MCP (Phase 1 audit)

What the engine and editor can already do, where it lives, and whether an AI client should reach it through MCP. Audited on `main` at `b3297ca` (2026-11-11). Source wins where docs disagree; every row names the code it was checked against.

Classes used below (defined in [architecture.md](architecture.md#7-safety-and-permissions)):

- **R** read-only
- **A** additive (creates something; undo removes it)
- **M** modifying, reversible (one undo step)
- **D** destructive or high-impact (needs a preview and confirmation)
- **—** not exposed

"Tool" names the proposed tool from [tool-inventory.md](tool-inventory.md).

## 1. What already exists that MCP should build on

These are the extension points the toolkit uses instead of writing its own.

| Existing piece | Where | What MCP gets from it |
|---|---|---|
| Instance arena with generational IDs | `core/src/arena.rs`, `core/src/world.rs` (`InstanceId::to_bits`) | Stable object references for the whole editor session. Delete only detaches, so an ID survives delete + undo (`editor/src/history.rs` header). IDs are **not** saved in `world.json`, so they don't survive a reopen; paths are the fallback. |
| Typed property set with validation | `World::set_property` → `PropertyError::{Stale, Unknown, …}` (`core/src/world.rs:1280`) | Unknown properties and wrong types are already rejected. MCP turns these errors into actionable messages. |
| Class reflection | `ClassName` (130 classes, `core/src/class.rs`), `default_properties`, `superclasses`, `events` | `class_info`, schema-free property discovery, "non-default only" output. |
| Property and API docs | `script/src/definitions.rs`: `property_description(class, prop)`, `api_reference()`; `PROPERTY_DOCS` (a test checks every property is documented) | `docs_search` and `class_info` text without hand-written MCP docs. |
| JSON value codec | `project/src/world_file.rs`: `json_to_value(json, kind)`, `json_value(value)` (both crate-private today) | One value format for MCP, the same as `world.json` (`[x,y,z]`, enum names). Needs making `pub`. |
| Change log | `World::record_changes`, `take_changes`, `next_change_number`, `Change::{Property, ChildAdded, ChildRemoved, Attribute, Tag}`; `Instance::revision`, `World::structure_version(_of)` | "What changed since cursor N" without re-sending the scene. |
| Undo history | `editor/src/history.rs`: `record`, `record_many`, `record_tree`, `record_files`, `record_terrain`, `record_world`, `record_edits` (diff of two worlds) | Every MCP write becomes one labelled undo step, using the same recorders as the UI. |
| Edit-world transaction pattern | Plugins (`docs/editor/plugins.md` §Undo, `app/plugins.rs`): copy world, run, diff with `record_edits`, merge until a waypoint | The exact model for `batch`: run ops on the edit world, roll back from the copy on failure, record one step. World clone cost measured at 11.5 ms for 10k Parts (PR #87). |
| Editor operations | `EditorApp::apply / apply_many` (`Edit` enum, `editor/src/properties.rs:24`), `act(Action)` (`explorer.rs:184`), `insert_named(Insertable, near, name, props)` (`app/tree_ops.rs`), `Insertable` (≈90 kinds, `editor/src/tree.rs`) | The "engine-native" path: Insert defaults, tree rules (what can be parented where, renamed, deleted), script-file renames, graph-script guards. |
| Command registry | `Command` enum (`app/command_palette.rs:127`), Ctrl+K `SearchIndex` of `SearchProvider`s (`app/search.rs`) | The list of user-level commands (Save, Undo, Insert(x), Play, Bake Probes, …). MCP doesn't expose the palette itself (it's UI navigation), but `Command` is the checklist of actions and the search providers are reused for `object_find`/`asset_find` ranking. |
| Headless editor | `EditorApp::new(Some(folder))` runs without a window in ~all editor tests; `main.rs` already has headless flags (`--cook`, `--build`) | A windowless MCP host with the same code paths as the editor. Play works headless (`app.play(); play_for(..)` in `app/agent_tests.rs`). |
| Project checks | `project::check_assets` (`validate.rs`), `asset_fix` plans, `asset_refs::find_references / unused_assets / plan_rename`, `side_warnings.rs`, `ScriptContext::check_syntax` (`script/src/context.rs:270`) | `project_check` without new validation code. |
| Version history | `project/src/versions.rs`: `record_checkpoint`, `list_versions`, `restore_version`, `version_changes` (`.axcenity/history/<id>/`) | A checkpoint before an AI session; a whole-session revert path that doesn't depend on undo. |
| Plugin sandbox and file rules | `plugins/src/files.rs` (project-relative paths only; refuses `..`, absolute, `\`, `:` and links out) | The same path rules for every MCP path parameter. |
| Game test runner | `testing::Runner`, `axcenity-test`, Tests panel (`editor/src/game_tests.rs`, runs on a thread, reports over a channel) | `test_run` for agent feedback loops. |
| Output log | `editor/src/output.rs` (script prints, errors with `path:line`, stack traces) | `play` returns errors and prints compactly. |

Nothing MCP-shaped exists yet: no JSON-RPC, no socket server, no `tokio` in the native build (only `wasm-bindgen-futures` lists it in `Cargo.lock`). `serde`/`serde_json` are workspace dependencies already.

## 2. Capability map by area

Columns: **Exists** (yes / partial / no), **Where**, **MCP** (class and tool), **Notes** (parameters that matter, what to return, safety).

### 2.1 Project and levels

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Project summary (name, description, format version, levels, counts, problems) | yes | `project::Project`, `read_manifest`, `load` | R `project_info` | Also the `axcenity://project` resource. |
| Create project (blank or genre template) | yes | `create_project`, `create_project_from`, `templates/` (arena, first-person, third-person, top-down, vehicle) | A `project_create` (full profile) | Writes a new folder; path must be inside an allowed root. |
| Open / switch project | yes | `EditorApp::open`, `close_project` | — in live mode (the user's choice); headless host takes `--project` | Switching projects under the user is surprising; leave it to the user. |
| Save | yes | `app/files.rs` `save`, `project::save_world` | D `project_save` | Overwrites files on disk. In live mode the user's Ctrl+S is the normal path; AI save is opt-in. |
| Levels: list, create, open, set start | yes | `project/src/levels.rs`, `editor/src/levels.rs` | R/A/M `level` | Opening a level swaps the edit world (unsaved check). |
| Version history: checkpoint, list, restore | yes | `project/src/versions.rs`, `app/version_history.rs` | A checkpoint (automatic), R list, D restore `history` | Restore replaces scripts/levels on disk. |
| Backup | yes | `project::back_up_project` | — | User feature. |
| Project settings: input map, collision groups, string table, developer console, loading screen, splash | yes | `project/src/input_map.rs`, `collision_groups.rs`, `strings.rs`, `set_*` | R in `project_info`; M `project_settings` (standard) | Edits already undo via `History::record_files`. |
| Build/export game (.axgame, platforms) | yes | `build_game.rs` (`--build`), `project::pack_game` | — (v1); maybe A `project_build` later | Long-running, writes outside the project. Not needed for editing. |

### 2.2 Scene hierarchy, objects, components

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Browse the tree | yes | `World::children`, `descendants_iter`, `full_name` | R `scene_tree` | Depth-limited, paginated, one line per object. |
| Find by class / tag / name / path | yes | `World::all_of`, `tagged_iter`, `find_first_descendant`; Ctrl+K object provider (`app/search.rs`) | R `object_find` | Index lookups are O(matches) (M2). |
| Read properties | yes | `Instance::properties`, `property` | R `object_get` | Default: non-default values only. |
| Create (Insert rules, default names, sensible defaults) | yes | `insert_named`, `Insertable` (~90 kinds incl. parts, lights, UI, audio, effects, characters, vehicles, nav agents) | A `object_create` | Reuse Insert defaults so AI-made objects match hand-made ones (e.g. BlueprintTexture on). |
| Set properties / name / parent | yes | `apply`, `apply_many`, `act(MoveInto/Rename)` | M `object_set` | Validation from `set_property` + tree rules (`tree.rs`). |
| Attributes and tags | yes | `Edit::Attribute/AddTag/RemoveTag`, `History::record_annotations` | M `object_set` (`attrs`, `tags`) | |
| Duplicate | yes | `act(Duplicate)`, `World::duplicate` (fixes internal references) | A `object_duplicate` | Optional offset and count. |
| Delete | yes (detach) | `act(Delete)` | D `object_delete` | Undoable, but scripts/graphs attached have guards (`graph_script_gui`). Preview + confirm above a threshold. |
| Group into Model/Folder | yes | Insert Model + `MoveInto` | A `object_group` (or `batch`) | |
| Lock | yes | `Action::SetLocked` | M `object_set` (`Locked`) | AI must respect Locked: refuse edits to locked objects unless the call unlocks them first. |
| Selection | yes | `EditorApp::selection`; plugin `Selection:Get/Set` | R/M `selection` (live mode only) | Lets the user say "this" and the AI see it. |
| Prefabs: save, place, overrides, apply/revert | yes | `app/prefabs.rs`, `core::prefab`, `Edit::{ApplyOverride,RevertOverride}` | A/M `prefab` (standard) | Apply-override rewrites the prefab file (D-ish: affects all copies). |
| Pivots | **no** | No `PivotOffset`/`WorldPivot` on Models | — | Missing capability (§3). |
| Components | n/a | Axcenity has no separate component type: behaviour is child Instances (Roblox model: a Light, Sound, Script, Health… inside a Part) | via `object_create` with `parent` | Document this mapping for agents used to Unity. |

### 2.3 Modeling and mesh editing

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Primitive parts (Block, Ball, Cylinder, Wedge) | yes | `Insertable::PARTS` | A `object_create` | |
| Solid modeling: Union, Negate, Intersect, Separate | yes | `app/solid.rs`, `mesh_edit/src/csg.rs` | A/M `solid_op` (standard) | One undo step; result is a `UnionOperation`. |
| Angular extend, weld, join surfaces | yes | `angular.rs`, `tree_ops::weld_selected/join_surfaces` | — v1 | Viewport-tool shaped; low AI value. |
| Model Studio ops (Extrude, Inset, Bevel, LoopCut, EdgeSlide, Knife, Bridge, Merge, Dissolve, Delete, Fill, GridFill), sculpt, paint, UV, rig, retopo, bake, LOD | yes | `mesh_edit/src/ops.rs` `MeshOp`, `model_studio.rs` | — v1 (listed as later) | Ops need face/edge selections by index; hard for any model to drive without vision. Revisit with `view_capture`. |
| Import model (glTF/FBX) | yes | `app/files/import.rs`, `import_dialog.rs`, `assets::import` | A `asset_import` | Undo removes copied files (`record_blobs_and_tree`). Source path outside project needs permission. |
| Generate LODs | yes | Model Studio | — v1 | |
| Fracture | yes | `Edit::GenerateFracture`, `mesh_edit/src/fracture.rs` | A via `object_action` | |

### 2.4 Materials and textures

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Built-in `Material` enum on parts, Color, Transparency, Metalness, Roughness, Emission | yes | `core/src/enums.rs`, Properties | M `object_set` | |
| Material files (`.material.json`, PBR params, maps, `base` inheritance, toon/outline/water shading) | yes | `assets/src/material.rs` `MaterialFile`, `app/material_files.rs`, Material tab | A `material_create`, M `material_set` | Assign = set `MaterialVariant` (+ Color white), one undo step (`apply_material`). |
| Material graphs | yes | `material_graph.rs` | — v1 | Node graphs are a poor fit for text tools; later. |
| Texture sets, prototype textures, Get texture (ambientCG) | yes | `texture_set.rs`, `prototype.rs`, `ambient_cg.rs`; Toolbox providers in PR #90 (`crates/toolbox`, not on `main` yet) | A `asset_get` (after Toolbox lands) | Network + licence: depends on the Toolbox thread's backend. |
| Cooked textures (BC7 mips) | yes | `cook` crate | — | Automatic. |
| Material compatibility errors | partial | `check_assets` finds missing maps; no shader-level validation | R `project_check` | |

### 2.5 Terrain and environment

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Generate terrain (presets, seed, size) | yes | `terrain/src/generate.rs` (`settings`, `generate`, `check`), Terrain Studio | A/M `terrain_edit` (`generate`) | Whole-landscape step (`TerrainChange`); replaces an existing landscape → D when one exists. |
| Brushes: Raise, Smooth, Flatten, Sculpt, Stamp, Noise, Paint | yes | `terrain/src/brush.rs` `apply`, `editor/src/terrain_studio.rs` `TerrainTool` | M `terrain_edit` (`brush`) | Params: tool, center, radius, strength, layer. Stroke = one step. |
| Erosion (hydraulic, thermal) | yes | `terrain/src/erosion.rs` | M `terrain_edit` (`erode`) | Background job (`app/terrain/jobs.rs`). |
| Layers and masks, auto paint | yes | `terrain_studio/layers.rs`, `TerrainLayer` class | M `terrain_edit` / `object_set` | |
| Foliage / vegetation scatter | yes | `TerrainTool::Foliage`, SplineScatter | M `terrain_edit` (`foliage`) | Grass density work deferred by Paul. |
| Flatten along spline, splines, SplineMesh/Scatter, Bake | yes | `procedural`, `Edit::{FlattenTerrainAlong,BakeSpline}` | A/M via `object_create` + `object_action` | |
| Lighting, time of day, latitude, fog | yes | `Lighting` service properties | M `env_set` (high-level) / `object_set` | |
| Sky image, Atmosphere, Clouds, Weather | yes | `app/sky.rs`, `Insertable::{Atmosphere,Clouds,Weather}` | A/M `env_set` | |
| Reflection/light probes, Bake Probes | yes | `Command::BakeProbes` | A `object_action` (`bake_probes`) | Writes `assets/probes`; one undo step. |

### 2.6 Animation

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Animate an object (Animator + clip + Animation) | yes | `Edit::Animate` | A `anim_edit` (`animate`) | |
| Clip files: property tracks, keyframes, interpolation (`Interp`), events | yes | `anim/src/files.rs` `ClipFile`, `PropertyTrack`, `PropertyKeyframe`, `ClipEvent` | M `anim_edit` (`key`, `unkey`, `event`) | File step (`record_files`). Track paths use `"."` for the root (`anim::target_path`). |
| Imported skeletal clips, takes | yes | `anim_import.rs` | A `asset_import` | |
| State machines: states, transitions, parameters, layers, blend trees/spaces | yes | `StateMachineFile` etc. (`anim/src/files.rs`), controller editor | M `anim_edit` (`state`, `transition`, `param`) (standard) | |
| IK (IKControl), ragdoll, foot placement | yes | `anim/src/ik_control.rs`, `Edit::BuildRagdoll` | A via `object_create` / `object_action` | |
| Sequences (cutscenes) | yes | `anim/src/sequence.rs`, `sequencer.rs` | — v1 (later `anim_edit` target) | |
| Retargeting | **no** | gap G16 | — | Missing capability. |

### 2.7 Physics and gameplay

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Part physics (Anchored, CanCollide, Massless, CustomPhysicalProperties, CollisionGroup) | yes | Part properties | M `object_set` | |
| Constraints/joints, movers, vehicles, seats | yes | `app/constraint_tool.rs`, `Insertable::{Vehicle,VehicleSeat,…}`, `Command::Constraint` | A `object_create` | Constraints need two attachments; high-level `connect` op in `batch` later. |
| Collision groups | yes | `project/src/collision_groups.rs` | M `project_settings` | |
| Scripts (.lua): create, write, rename, delete, server/client/module | yes | `project::create_script/write_script/rename_script/delete_script`, `InsertScript` | A `script_create`, M `script_edit`, D `script_delete`, R `script_read` | Writes are file steps (undo). Return syntax check (`check_syntax`) and side warnings. |
| Block scripts, UI event graphs, behavior trees | yes | `blocks`, `ui_graphs`, `behavior_trees` | R (as Lua translation) — writes later | Visual tools; agents should write Lua. |
| Gameplay kits: Health, DamageZone, Ability, Effect, Checkpoint, Tool, Inventory, ProximityPrompt, ClickDetector, Characters, NavigationAgent | yes | `Insertable` | A `object_create` | High-level `character_add` in `batch` presets. |
| Navigation mesh, agents | yes | `nav` | R in `project_check`; A via `object_create` | |
| Input map | yes | `project/src/input_map.rs` | M `project_settings` | |
| Data tables, dialogues/quests, string table | yes | `data_table.rs`, `dialogue_editor.rs`, `strings.rs` | M `data_edit` (later) | File-backed JSON; simple to add once the core lands. |
| Script debugging (breakpoints, locals) | partial | `script/src/debugger.rs` (no stepping, G6) | — v1 | |

### 2.8 Audio

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Sound, SoundGroup, sound effects, ReverbZone | yes | `Insertable::AUDIO`, `SOUND_EFFECTS`, `audio` crate | A `object_create`, M `object_set` | Assign `SoundId` to a project path. |
| Spatial audio, roll-off distances | partial | Linear roll-off only; `RollOff*` distances exist | M `object_set` | `RollOffMode`, occlusion and material interactions **missing** (G8). |
| Import audio | yes | Assets panel (copy into `assets/audio`) | A `asset_import` | |
| Preview sound | yes | `Edit::PreviewSound` | — | No value to a remote agent. |

### 2.9 UI

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| UI Instances (ScreenGui, Frame, TextLabel, TextButton, ImageLabel, TextBox, PageLayout, Page, Table, ScrollingFrame, Slider, Toggle, Dropdown, ProgressBar, BillboardGui, HealthBar) | yes | `Insertable::UI`, `ui` crate | A `object_create`, M `object_set` | |
| Layout: CSS (taffy flexbox/grid), classes, ids, transitions | yes | `ui/src/css.rs`, stylesheets, UI Designer (`ui_designer.rs`, `style_panel.rs`) | M `style_edit` (standard) | No `AnchorPoint`: layout is CSS. Document this. |
| Events | yes | Lua (`Activated`, …) and UI event graphs | via `script_edit` | |
| Localization | yes | string table, `TextKey` | M `data_edit` (later) | |

### 2.10 Lighting and rendering

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Lights (Point, Spot, Rect), shadows, emission | yes | `Insertable`, Lighting `ShadowsEnabled` | A/M `object_create`/`object_set`, `env_set` | |
| Post: ColorCorrection, Bloom, Exposure, Fog, lens effects, GraphicsVolume, Outline | yes | `Insertable::{ColorCorrectionEffect,…}` | A/M `env_set` | |
| Graphics quality settings | yes, per user | `render-types` `GraphicsSettings` (quality, shadows, AO, GI, …) — editor/user setting, not project data | R `render_stats`; M only in full profile | Changing the user's settings is surprising; read-only by default. |
| Diagnostics: frame/pass times, draws, triangles, GPU memory | yes | `RenderStats`, `profiler.rs`, `ui::render_lines` | R `render_stats` (live mode / Play) | Headless host has no GPU frame. |
| Viewport screenshot | partial | `editor/src/snapshot.rs` (tests, offscreen), `render` offscreen in `every_example_renders` | R `view_capture` (later, needs GPU) | Big value for multimodal agents; costs image tokens. |

### 2.11 Assets

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| List/search project assets by kind | yes | `asset_browser/scan.rs`, `project::list_project_files`, `files_of_kind` | R `asset_find` | |
| Asset metadata (size, kind, references) | partial | `asset_refs::find_references`, `assets/src/asset_id.rs` | R `asset_find` (`detail`) | Licence/source records come with Toolbox `metadata.db` (#90/#92). |
| Missing references, unused assets, fixes | yes | `check_assets`, `unused_assets`, `asset_fix::plan_fix` | R `project_check`; M `asset_fix` (later) | |
| Rename/move with reference updates | yes | `plan_rename` | M `asset_move` (standard) | One undo step. |
| Delete assets | yes | `app/assets.rs` `delete_assets` (bytes kept for undo) | D `asset_delete` | Preview lists referencing objects. |
| Global asset library | yes | `global_assets.rs` | A `asset_get` (later) | |
| Toolbox providers (ambientCG v3, Poly Haven, Openverse, …) | in progress | PR #90, `crates/toolbox` (Toolbox thread) | R `asset_search_online`, A `asset_get` | Owned by the Toolbox thread; MCP only wraps its API once it's on `main`. |

### 2.12 Play, tests, diagnostics

| Capability | Exists | Where | MCP | Notes |
|---|---|---|---|---|
| Play / Stop in the editor | yes | `EditorApp::play/stop`, `session.rs` | M `play` (run N seconds, return output) | Runs game scripts: scripts are already sandboxed (`script/src/sandbox.rs`). Time cap. |
| Output log, script errors with stack | yes | `output.rs` | R `play`, `log_read` | |
| Game tests (`tests/*.spec.lua`, JUnit) | yes | `testing::Runner` | R `test_run` | Runs against saved files: needs save first (state it in the result). |
| Profiler (Game/Graphics) | yes | `profiler.rs` | R `render_stats` | |
| Developer console Lua | yes | `console.rs`, `dev_console.rs` | **— (deliberately)** | Arbitrary Lua is the "unrestricted command interface" the brief rules out. See decision D6. |
| Editor plugins (Lua, edit world) | yes | `plugins` crate | — | MCP is a sibling of plugins, not built on them, but shares the undo pattern. |

## 3. Missing capabilities (don't fake them)

Requested in the brief but not in the engine today. MCP reports them in `project_info.capabilities.missing` so an agent doesn't try:

| Missing | Status | Owner / ID |
|---|---|---|
| Model pivots (`PivotOffset`, `WorldPivot`) | not planned | — (new gap) |
| UI anchors (`AnchorPoint`) | by design: CSS layout instead | — |
| Audio occlusion, `RollOffMode`, material interactions | planned | G8 |
| Animation retargeting | planned | G16 |
| Debugger stepping | planned | G6 |
| In-editor Lua completion/diagnostics (luau-lsp) | waits for M12 | G4 |
| Scripts inside objects | design note #91 | G2 |
| Model Studio ops by text (face/edge selection) | exists in UI only | later MCP phase |
| Material graph editing by text | exists in UI only | later MCP phase |
| Physics debug views as data | planned | G15 |
| Run-mode (simulate without player) | planned | G14 |

## 4. What cannot be exposed safely (and why)

- **Arbitrary Lua in the edit world** (Console, plugins): can change anything, read project files, loop forever. Not an MCP tool (D6).
- **File access outside the project**: only `asset_import` reads outside, from folders the user allows; writes never leave the project (plugin path rules).
- **Editor/user settings, layouts, theme**: the user's, not the project's. Read-only at most.
- **Open/close/switch project in a live editor**: the user's session; MCP works on the open project.
- **Build/export, packaging, GitHub releases**: long-running, writes outside the project. Later, opt-in.
- **Network (Toolbox downloads)**: only through the Toolbox backend once it lands, with its licence records.

## 5. Architecture constraints found

- `editor` is a **binary crate** (no `lib.rs`), so tools that call `EditorApp` must live inside it (`editor/src/mcp/`). Protocol code that doesn't need the editor can be its own library crate.
- All editor state lives on the egui main thread (`EditorApp`); a server thread must hand requests to the frame loop (channel + `egui::Context::request_repaint`), as the Tests panel and file watcher already do.
- No async runtime in the native build. A stdio JSON-RPC loop needs none.
- Instance IDs are per session; `world.json` has no IDs. Paths (`Workspace/House/Door`) are the cross-session reference, IDs the in-session one.
- There is no project lock: an editor and a headless host could both open a folder. The design adds one (architecture §3.4).
