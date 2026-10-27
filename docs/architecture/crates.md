# Crates

Which crate owns what, and what it may depend on. Read this before adding a crate or a dependency.

| Crate | Owns | May depend on |
|---|---|---|
| `core` | Instance tree, arena, properties, events, services | nothing engine-internal |
| `assets` | the asset system: loading glTF/FBX models (with FBX skeletons, skin weights and takes), posing them, reading project files (`Assets`) and importing models | nothing engine-internal; plain data out |
| `anim` | animation: clip and animator controller files, state machines and blending, and `Animators`, which plays every Animator and poses its MeshPart | `assets`, `core` |
| `audio` | Kira playback for project sounds: `GameAudio` applies queued `SoundAction`s, keeps playback in line with each Sound, and spatializes emitters against the camera | `assets`, `core` |
| `blocks` | persisted block-script data, node graphs (the UI Event Graph, and the nodes behavior trees offer) and their one-way Lua translation | nothing engine-internal (`core` and `script` only in tests) |
| `render` | `wgpu` renderer | nothing engine-internal; takes a draw list |
| `scene` | Instance tree → draw list (`build_draw_list`, including terrain chunks and UI layout), the shared `FlyCamera`, viewport picking (`pick`) | `core`, `assets`, `render`, `terrain`, `ui` |
| `terrain` | terrain algorithms over `core::TerrainData`: brushes, generation, erosion, chunk meshes, file codecs | `core` |
| `physics` | `rapier3d` integration | `core`, `assets` |
| `character` | kinematic third-person walk and run; queries physics, never the reverse | `core`, `assets`, `physics` |
| `nav` | the navigation mesh, built from `physics::solids`, paths across it, the `NavigationAgent` stepper, and behavior trees (checked and run, read from their files with `serde_json`); neither `physics` nor `character` may depend on it | `core`, `assets`, `physics`, `character` |
| `save` | save slots and checkpoints: `Saves` (player data, settings, the active checkpoint), world state snapshots and their restore, slot files in the player's data for the game (`game_folder`); never the project folder | `core` |
| `script` | Luau (`mlua`), sandbox, scheduler, engine API, TweenService (eased by `anim`'s curves), SaveService (over `save`; places the player character found by `character`), UserGameSettings (holds `render`'s plain `GraphicsSettings` and `Capabilities` data, nothing else of it) | `anim`, `audio`, `character`, `core`, `physics`, `assets`, `render`, `save` |
| `ui` | UI Instance layout, CSS cascade (`taffy`), transitions (eased by `anim`'s curves) and screen-space draw data | `anim`, `core`, `render` |
| `project` | project folder load and save, including CSS and block-script sources, upgrading older projects (animation files through `anim`), and the editor's asset check | `anim`, `assets`, `blocks`, `core`, `terrain` |
| `player` | runtime shipped with games (binary) | `anim`, `audio`, `character`, `core`, `assets`, `nav`, `render`, `save`, `scene`, `physics`, `script`, `project`, `ui` |
| `editor` | `egui` editor app (binary) | `anim`, `audio`, `blocks`, `character`, `core`, `assets`, `nav`, `physics`, `project`, `render`, `save`, `scene`, `script`, `terrain`, `ui` |

Block scripts are translated to Lua at project load and run through the same `ScriptRuntime` as handwritten scripts. Hosts hand queued sound actions to their `GameAudio` after script steps, and fire `Ended` for the sounds it reports finished; the player owns playback for its lifetime, while editor Play owns a separate `GameAudio` that is dropped on Stop.

Rules:
- `player` must **never** depend on `editor`, `egui` or `blocks`.
- `render` never sees Instances, scripts or the editor. Something outside it turns the world into a draw list (meshes, materials, transforms, lights, camera).
- `ui` reads the Instance tree and adds screen-space layout data to the render draw list; `render` remains Instance-free.
- Don't let crates depend on each other in a cycle. If two crates need each other, move the shared type into `core`.
- Pin dependency versions in the workspace's root `Cargo.toml` (`[workspace.dependencies]`).

## Where the big files split

A large module is a parent file plus a folder of the same name. The parent keeps the types and the entry points; each child holds one responsibility, as another `impl` block or as free functions, and starts with `use super::*;`. A `tests.rs` beside the parent holds its tests. Library crates re-export moved public items with `pub use`, so paths such as `axcenity_project::create_script` did not change.

| Parent | Children |
|---|---|
| `editor/src/app.rs` | `assets`, `behavior_trees`, `chrome`, `commands`, `edits`, `files`, `session`, `settings`, `terrain`, `tree_ops`, `ui_graphs` |
| `app/chrome.rs` | `panes` (dock panels), `menus` (title and menu bar), `toolbar` (ribbon and tools), `windows` (shortcuts, versions, pause and overlay widgets) |
| `app/files.rs` | `export`, `import` (models and animations), `pickers`, `script_files`, `watch` (files changed on disk) |
| `app/terrain.rs` | `strokes`, `layers`, `jobs` (generate and erode), `disk` |
| `editor/src/animation.rs` | `documents` (open, close, follow), `chrome` (show, breadcrumb, outline), `clip_ui`, `machine_ui`, `dialogs`; the clip and state-machine editors are `animation/clip_editor.rs`, `property_editor.rs` and `controller_editor.rs` (each with its own children) |
| `animation/posing.rs` | `tree` (the drawer's bone tree and folds) |
| `editor/src/asset_browser.rs` | `packages` (model folders), `scan` (finding asset files and their kinds), `thumbnails` |
| `editor/src/block_editor.rs` | `painter` (drawing blocks), `palette`, `canvas` (canvas, dragging, Lua preview), `menus` |
| `editor/src/node_graph.rs` | `draw` (nodes, pins, values), `menu` (the node menu), `variables` |
| `editor/src/terrain_studio.rs` | `tool_settings`, `layers`, `presets` |
| `editor/src/start.rs` | `dialogs`, `cards`, `pages`, `global` |
| `editor/src/viewport.rs` | `input` (camera, picking, dragging), `overlays` (selection, lights, brush, debug) |
| `render/src/renderer.rs` | `types` (GPU structs and constants), `scene` (`ScenePipeline`, the frame), `gpu`, `lighting`, `post`, `sprites`, `terrain`, `ui_pass`, `gpu_timer` |
| `project/src/lib.rs` | `templates`, `scripts`, `stylesheets`, `ui_graphs` (plus `world_file`, `migrate`, `packed`, `versions`, `validate`, `terrain_files`) |
| `ui/src/lib.rs` | `css` (property parsing), `paint` (draw items), `style` |
| `blocks/src/lib.rs` | `translate` (Lua output), `from_lua`, `lua`, `node_graph` (generic graphs), `ui_graph` (UI nodes and their Lua), `behavior_tree` (the nodes behavior trees offer) |
| `script/src/definitions.rs` | `tables` (types, functions), `docs` (property, event and class text) |
