# Plan: Axcenity Engine v1

> Source PRD: `prd/axcenity-engine-v1.md`. Milestone 0 (learning) has no code and is not included here. This plan starts at Milestone 1.

## Architectural decisions

These apply to every phase. Details live in `docs/*.md`.

- **Workspace**: one Cargo workspace with these crates: `core`, `render`, `physics`, `script`, `blocks`, `ui`, `audio`, `project`, `player` (binary) and `editor` (binary). A crate is created in the first phase that needs it, not before.
- **Dependency rules**:
  - `player` never depends on `editor`, `egui` or `blocks`.
  - `render` receives only a draw list and never sees Instances or scripts.
  - No cycles between crates; shared types live in `core`.
- **Libraries**: `winit`, `wgpu`, `rapier3d`, `mlua` (Lua 5.4, vendored), `egui`, `taffy`, a CSS parser, `cosmic-text`/`glyphon` and `kira`. All are pinned in the workspace dependencies and must have Apache-2.0-compatible licenses.
- **Data model**: one tree of Instances (class, `Name`, `Parent`, children, typed properties and events), stored in an arena with generational IDs. `game` is the root, and `Workspace` holds the 3D world.
- **Materials**: PBR fields (base color, metalness, roughness, emissive, opacity) from the first rendered frame.
- **Scripting**:
  - Lua 5.4 in a per-script sandbox, with a Roblox-familiar API.
  - Every script and handler runs as a coroutine owned by the scheduler.
  - `Script` runs on the server side, `LocalScript` on the client side, and `RemoteEvent` connects them over an in-memory loopback.
  - Blocks always translate to Lua.
- **Frame order**:
  1. Input
  2. Resume scripts and fire queued events
  3. Physics at a fixed 60 Hz
  4. Touch and collision events
  5. UI layout
  6. Render
- **Project format**:
  - a folder with a manifest and readable world files
  - scripts as `*.server.lua` / `*.client.lua`
  - block scripts as their own files
  - `.css` stylesheets
  - plain asset files
- **Platforms**: Windows, macOS and Linux. CI builds and tests all three from Phase 1 onward. A phase isn't done unless CI is green on all three.
- **License**: Apache-2.0 with a `NOTICE` file.

---

# Milestone 1: End-to-end slice

## Phase 1: Window on all three OSes

**User stories**: 60, 58

### What to build

The workspace skeleton with the `player` crate. Running the player opens a resizable window and clears it to a sky color each frame using `wgpu`. Add a license and `NOTICE`, a README with the name's pronunciation, and CI that builds, lints and tests on Windows, macOS and Linux.

### Acceptance criteria

- [x] `cargo run` for the player opens a sky-colored window on Windows, macOS and Linux.
- [x] Resizing and closing the window work without errors or panics.
- [x] CI runs build, clippy (warnings are errors), fmt check and tests on all three OSes, and passes.
- [x] Apache-2.0 license, `NOTICE` and README are present.

---

## Phase 2: One lit part from a project folder

**User stories**: 1, 39, 49

### What to build

Create the `core`, `project` and `render` crates. The player takes a project folder path, reads its manifest and world file, and builds an Instance tree (`game` → `Workspace` → one `Part`) in the arena. A step outside `render` turns the tree into a draw list. `render` draws the part as a PBR-shaded block lit by a sun and ambient light, viewed from a fixed camera. Choose the world file format (RON or JSON) here and record it in `docs/project-and-export.md`.

### Acceptance criteria

- [x] An example project folder in the repository loads, and a single lit block appears.
- [x] Changing the part's color, metalness or roughness in the world file changes how it looks on the next run.
- [x] A missing or malformed world file produces a clear error message, not a panic.
- [x] Unit tests cover the arena: create, parent, reparent, delete, and detecting stale IDs.
- [x] `render` has no dependency on `core`'s Instance types; it only receives the draw list.

---

## Phase 3: Primitives, materials and a camera

**User stories**: 2, 10

### What to build

Support block, sphere, cylinder and wedge shapes. Parts have `Position`, `Size`, `Orientation`, `Color` and `Material` properties, and material presets (plastic, metal, glass, neon) map onto the PBR fields. Add a free-fly camera to the player (WASD plus mouse look) so you can inspect the scene.

### Acceptance criteria

- [x] An example scene with all four shapes in different materials renders correctly.
- [x] Glass is see-through and neon glows (emissive), within stage 1 lighting.
- [x] The fly camera moves smoothly and independently of frame rate.
- [x] Property values round-trip: load → save → load gives an identical world file.

---

## Phase 4: First Lua script

**User stories**: 15, 19, 22

### What to build

Create the `script` crate. When the project loads, each `*.server.lua` file becomes a `Script` Instance and runs in its own sandboxed environment. Expose the first slice of the Roblox-familiar API: `game`, `workspace`, `Instance.new`, `FindFirstChild`, reading and writing part properties, and `Vector3`/`Color3`. `print` and runtime errors (with script path and line) go to the console.

### Acceptance criteria

- [x] An example script creates a new part and moves an existing one, and the changes appear on screen.
- [x] `io`, `os.execute`, `debug`, `load`, `dofile` and `loadfile` are unavailable inside scripts (covered by tests).
- [x] A script with a runtime error reports its path and line, and other scripts keep running.
- [x] Scripts can't access each other's global variables.

---

## Phase 5: Task scheduler

**User stories**: 17

### What to build

Run every script body as a coroutine owned by a scheduler. Implement `task.wait`, `task.spawn` and `task.delay`. Put the frame loop into the fixed order from the architectural decisions, with script resumption at step 2.

### Acceptance criteria

- [x] A script that loops forever with `task.wait(1)` and toggles a part's color does so once per second without freezing the window.
- [x] `task.wait` returns roughly the requested time, and a test checks it against a simulated clock.
- [x] `task.spawn` and `task.delay` run in a predictable order, verified by a test.
- [x] An error inside a spawned task is reported and doesn't kill its parent script.

---

## Phase 6: Physics

**User stories**: 12, 37

### What to build

Create the `physics` crate. Parts have an `Anchored` property: anchored parts are fixed colliders, and unanchored parts are dynamic bodies. Physics steps at a fixed 60 Hz using an accumulator, and positions sync back into the Instance tree after each step. Scripts that set `Position` on a dynamic part teleport it.

### Acceptance criteria

- [x] In the example scene, unanchored parts fall, collide and come to rest on an anchored floor.
- [x] Physics behaves the same at 30 fps and 144 fps (the fixed step is verified by a test).
- [x] Toggling `Anchored` from a script freezes or releases a part.
- [x] A part deleted from the tree is removed from the physics world.

---

## Phase 7: Touched events

**User stories**: 16, 38

### What to build

Add a general event/signal system with `:Connect` that returns a connection you can `:Disconnect`. Contacts from `rapier` fire `Touched` on both parts at frame-order step 4, and each handler runs as a scheduler coroutine. This completes the Milestone 1 demo: a ball drops onto a pad, and a script changes the pad's color and prints the ball's name.

### Acceptance criteria

- [x] The Milestone 1 demo project runs end-to-end on all three OSes.
- [x] `Touched` fires once per new contact, not every frame while touching.
- [x] `:Disconnect` stops further calls to the handler.
- [x] A handler that calls `task.wait` doesn't block other handlers or the frame.

---

# Milestone 2: Minimal editor

## Phase 8: Editor shell

**User stories**: 6, 22

### What to build

Create the `editor` crate, an `egui` app that opens a project folder and shows a 3D viewport (the same renderer, with an orbit/fly camera), an Explorer panel showing the Instance tree, and an Output panel. The editor only displays in this phase; there's no editing yet. (Added during the phase: adjustable fly speed with Shift+wheel or the wheel while flying, shown in a fading pop-up.)

### Acceptance criteria

- [x] The editor opens the Milestone 1 demo project and shows it in the viewport.
- [x] The Explorer shows `game → Workspace → …`, and branches expand and collapse.
- [x] Load errors and messages appear in the Output panel.
- [ ] The editor runs on all three OSes, and the player still builds without `editor` or `egui`.

---

## Phase 9: Select and edit properties

**User stories**: 5, 8, 49

### What to build

Clicking a part in the viewport (ray pick) or in the Explorer selects it, and the selection is highlighted in both places. The Properties panel shows the selected Instance's properties with type-appropriate controls (numbers, vectors, color picker, material dropdown, checkboxes). Edits apply live. Save writes the project folder.

### Acceptance criteria

- [x] Selecting in either place highlights the same Instance in both.
- [x] Editing Position, Size, Color, Material or Anchored updates the viewport immediately.
- [x] Save, close and reopen reproduces the edits exactly.
- [x] Unsaved changes show an indicator, and closing asks before discarding them.

---

## Phase 10: Transform tools and undo

**User stories**: 4, 13

### What to build

Add move, scale and rotate gizmos in the viewport, with configurable grid and angle snapping. All edits, including Properties panel changes, go through a command history that supports undo and redo.

### Acceptance criteria

- [x] Each gizmo moves, scales or rotates the selected part along the chosen axis.
- [x] Snapping can be turned on and off, and the step size can be changed.
- [x] Undo and redo work for gizmo drags (one step per drag) and property edits.
- [x] Undo history survives saving.
- [x] Dragging a part by its body (added on request, like Studio) places it on the surface under the mouse, snapped to the grid when move snapping is on, as one undo step.

---

## Phase 11: Tree editing

**User stories**: 2, 7, 9

### What to build

Insert parts and other Instances from a menu, and rename, delete and duplicate them. Drag in the Explorer to reparent. Add `Model` and `Folder` classes for grouping. All of these actions are undoable.

### Acceptance criteria

- [x] Each primitive can be inserted at the camera's focus point.
- [x] Dragging in the Explorer reparents, and invalid moves (into your own descendant) are rejected.
- [x] Deleting a Model deletes its children, and undo restores all of them with the same properties.
- [x] Inserting a new Instance gives it a unique default name among its siblings.

---

## Phase 12: Play and Stop

**User stories**: 31, 32, 33, 34, 36

### What to build

Add Play and Stop buttons. Play autosaves, deep-copies the edit world, and runs scripts and physics on the copy in the viewport. During Play, the Explorer and Properties panels show the live world. Stop discards the copy and restores the edit world. Rust panics during Play are caught at the Play boundary and reported in Output.

### Acceptance criteria

- [x] Play runs the Milestone 1 demo inside the editor.
- [x] After Stop, every Instance and property matches the state before Play (verified by a test).
- [x] The project is saved to disk before Play starts.
- [x] A deliberately triggered engine panic during Play shows an error and returns to edit mode without closing the editor.

---

## Phase 13: Script editing

**User stories**: 21, 51, 52

### What to build

Add a script editor tab with Lua syntax highlighting, and an "Insert Script" action that creates the Instance and its `.lua` file together. The editor watches the project folder and reloads changes from outside editors. A file that fails to load is reported and skipped rather than blocking the project.

### Acceptance criteria

- [x] Creating, editing and saving a script in the editor writes the matching `.lua` file.
- [x] Editing that file in VS Code while the editor is open updates the editor within about a second.
- [x] A corrupted world file or script is reported in Output, and the rest of the project still loads.
- [x] Script errors in Output show the path and line, and clicking one opens the script at that line.

---

# Milestone 3: Scripting depth

## Phase 14: Server and client split

**User stories**: 14, 18, 23

### What to build

`*.client.lua` files become `LocalScript` Instances. They run in a separate client context, connected to the server context through a loopback transport hidden behind a trait. Add `RemoteEvent` (`FireServer`, `FireClient`, `FireAllClients` and their events) and client-only input and camera APIs. Using a client-only API from a `Script`, or the reverse, raises a helpful error, and the editor shows a warning badge.

### Acceptance criteria

- [x] An example game reads keyboard input in a `LocalScript`, sends it through a `RemoteEvent`, and a `Script` moves a part in response.
- [x] Calling an input API from a `Script` gives an error that explains to use a `LocalScript` instead.
- [x] The loopback transport can be swapped out (proven by a test double).
- [x] Server and client contexts don't share Lua globals.

---

## Phase 15: Watchdog and sandbox hardening

**User stories**: 35

### What to build

Add an instruction-count hook that stops a coroutine that runs too long without yielding and reports the script and line. Review the sandbox: per-script environments, a safe `require` for project modules, and limited `collectgarbage`. Add tests that attempt to escape the sandbox.

### Acceptance criteria

- [x] `while true do end` during Play is stopped with an error, and the editor stays responsive.
- [x] Normal heavy scripts (for example, loops of a few million iterations that yield) aren't falsely stopped.
- [x] A suite of sandbox-escape attempts all fail.
- [x] `require` loads project modules and nothing outside the project.

---

## Phase 16: More events and autocomplete

**User stories**: 16, 20

### What to build

Add `Changed` and property-change signals, `ChildAdded`/`ChildRemoved`, and a heartbeat-style per-frame event. Build a generator that writes a Lua Language Server definitions file from the engine's API descriptions, and an editor action that drops it into the project so VS Code gets autocomplete.

> The generator (`crates/script/src/definitions.rs`) and its up-to-date test were built early, right after Phase 4. This phase extends it with the new events and `task.*`, and adds the editor action.

### Acceptance criteria

- [x] `Changed` fires when a property changes, from scripts, physics or the editor during Play.
- [ ] Opening the project in VS Code with Lua Language Server gives autocomplete for `Instance.new`, part properties and `task.*`.
- [x] CI fails if the generated definitions are out of date with the API.

---

## Phase 17: glTF import

**User stories**: 3

### What to build

Add an import action that copies a `.gltf`/`.glb` file into the project's assets and creates a mesh-part Instance that references it. The mesh renders with its material, mapped onto the PBR fields, and gets a collision shape (a convex hull, or the bounding box as a fallback).

### Acceptance criteria

- [x] A Blender-exported glTF model imports, renders, and can be moved, saved and reloaded.
- [x] An imported mesh collides with physics parts.
- [x] A malformed glTF file produces a clear error and doesn't crash.

---

## Phase 18: Choose objects on import

**User stories**: 61

### What to build

Importing a glTF or FBX file with more than one mesh object opens a dialog. It shows the file's node tree with a checkbox, triangle count and material names for each mesh node, and ticking a group ticks its children. Choose how the ticked objects land: one `Model` holding a `MeshPart` per object, laid out as in the file; separate `MeshPart`s at the insert point; or one merged `MeshPart`, as before. A `MeshId` can name one object inside a model file (`…/props.fbx#Street/Bench`), so every part shares one copied file. Files with a single object skip the dialog.

### Acceptance criteria

- [x] Importing a multi-object FBX (such as a city props pack) lists its objects, and importing a few of them creates only those.
- [x] Each placement choice produces the expected Instances, and the Model layout matches the file.
- [x] A part naming one object saves, reloads and collides with its own shape, not the whole file's.
- [x] Cancelling the dialog imports nothing and copies no files.

---

## Phase 19: PBR texture maps

**User stories**: 62

### What to build

The draw list's material gains normal, metalness, roughness, ambient occlusion, emission and opacity maps, and the renderer uses them (normal maps with tangents, AO darkening ambient light). glTF models use their own normal, metallic-roughness, occlusion and emissive textures. FBX models use every map their materials link, including Phong/Lambert ones (diffuse, specular, glossiness, bump-as-normal, transparency), converted to metallic-roughness.

### Acceptance criteria

- [ ] A glTF sample with all its maps (such as DamagedHelmet) renders with visible normal detail, shiny metal and glowing parts.
- [ ] An FBX with linked base color, normal, roughness, metalness, emission, opacity and AO maps renders with each one applied.
- [ ] A glossiness map renders as the inverse roughness; a specular map changes how strongly non-metal parts shine.
- [x] Built-in parts look the same as before.

---

## Phase 20: Embedded and channel-packed textures

**User stories**: 63

### What to build

Textures embedded in an FBX are used and copied out as image files on import. Each texture's role is taken from the material link, or else guessed from its name (`albedo`, `basecolor`, `diffuse`, `normal`, `nrm`, `rough`, `gloss`, `smoothness`, `metal`, `spec`, `emissive`, `opacity`, `ao`, `occlusion`, and common suffixes such as `_N`, `_AO`). Channel-packed maps are split by known layouts: ORM/ARM, RMA, Unity's metallic + smoothness in alpha, HDRP's mask map, and opacity in base color alpha.

### Acceptance criteria

- [x] An FBX with embedded textures imports with them applied, and they appear as files in the model's folder.
- [x] An ORM-packed texture and a Unity metallic-smoothness texture each drive the right channels.
- [x] A texture with an unknown role is reported in Output and left out, and the model still loads.

---

## Phase 21: SurfaceAppearance and texture overrides

**User stories**: 64

### What to build

Each `MeshPart` gets one `SurfaceAppearance` child per material in its file, holding that material's maps (`ColorMap`, `NormalMap`, `MetalnessMap`, `RoughnessMap`, `EmissiveMap`, `AmbientOcclusionMap`, `OpacityMap`) as project texture paths, plus per-map settings (source channel, invert for glossiness, DirectX or OpenGL normals, emissive strength). Properties edits them, with a texture file picker. The import dialog lists each material's detected maps, so they can be changed before importing. Scripts can read and set the same properties.

### Acceptance criteria

- [x] Importing creates a SurfaceAppearance per material with the detected maps filled in.
- [x] Changing a map or setting in Properties updates the viewport straight away, and it saves and reloads.
- [x] Changing a material's maps in the import dialog is what gets imported.
- [x] Deleting a SurfaceAppearance draws that material with the file's own textures again.

---

# Milestone 4: Game UI

## Phase 22: UI Instances on screen

**User stories**: 42

### What to build

Create the `ui` crate. Add `ScreenGui`, `Frame` and `TextLabel` Instances. `taffy` computes layout at frame-order step 5, text is shaped with a text library, and `render` draws the UI as an overlay from UI draw data. Built-in default styles only; no CSS yet.

### Acceptance criteria

- [x] A `ScreenGui` with a `Frame` holding a `TextLabel` appears over the 3D view in the player and in the editor's Play mode.
- [x] A script setting `Text` updates the label on the next frame.
- [x] Layout adapts when the window is resized.

---

## Phase 23: CSS stylesheets

**User stories**: 43, 44, 45

### What to build

Load `.css` files from the project. Support the subset in `docs/ui.md`: type, `.class`, `#id` and descendant selectors with standard specificity, flexbox properties, sizes in px and %, padding, margin, colors, borders, `border-radius`, fonts, opacity and CSS variables. UI Instances get `Class` and `Id` properties. Unsupported properties produce warnings in Output.

### Acceptance criteria

- [ ] An example HUD (a health bar and score) styled only with CSS matches a reference screenshot.
- [x] Specificity and cascade tests pass for the supported selectors.
- [x] CSS variables resolve, including fallbacks.
- [x] An unknown property logs a warning, and the rest of the stylesheet still applies.

---

## Phase 24: Interactive UI

**User stories**: 46, 47

### What to build

Add `TextButton` and `ImageLabel`. Support `:hover` and `:active` states, button `Activated` events delivered to `LocalScript`s, and `Visible`. Changes to UI Instances or `.css` files show live in the editor viewport while editing, not only during Play.

### Acceptance criteria

- [x] An example menu with buttons that change color on hover and press, and a script that reacts to clicks.
- [x] Editing a `.css` file updates the editor viewport without restarting.
- [x] Hidden (`Visible = false`) elements take no space in layout and receive no clicks.

---

# Milestone 5: Blocks

## Phase 25: First blocks

**User stories**: 24, 25, 26, 29

### What to build

Create the `blocks` crate and a block editor panel. Start with a small palette: "when this part is touched," "when game starts," "set property to value," "wait N seconds," "move to," "if/else," "repeat," variables and numbers. Shapes only accept compatible blocks. Block scripts save as their own files and translate to Lua, which runs exactly like a hand-written script.

### Acceptance criteria

- [ ] A beginner can rebuild the Milestone 1 "touch pad changes color" demo using only blocks.
- [x] Incompatible blocks can't be dropped into a slot.
- [x] Translation tests cover every block type, and the output runs in the scheduler.
- [x] Every block maps to a documented Lua API; there are no block-only features.

---

## Phase 26: Blocks as a learning path

**User stories**: 27, 28, 30

### What to build

Add a "View as Lua" panel showing readable generated code. Generated Lua carries source-block tags, so runtime errors highlight the responsible block in red. Each block script shows a server or client badge, and client-only blocks are only offered in client scripts.

### Acceptance criteria

- [x] "View as Lua" output is readable: indented, with meaningful variable names and no internal noise.
- [x] A runtime error in a block script highlights the block that caused it.
- [x] The badge matches where the script runs, and client-only blocks are hidden in server block scripts.

---

# Milestone 6: Export

## Phase 27: Export for your own OS

**User stories**: 54

### What to build

Add an editor export action: translate blocks to Lua via `blocks`, then `project` packs the project into one game file. The player detects a packed game file next to itself, or embedded in itself, and runs it without a project folder.

### Acceptance criteria

- [ ] An exported demo game runs on the same OS from a clean folder with no editor installed.
- [x] The exported game contains no editor code (verified by a dependency check in CI).
- [ ] Block scripts in the exported game behave the same as in Play.

---

## Phase 28: Export for every OS

**User stories**: 53, 55, 56, 57, 59

### What to build

CI builds release players for Windows, macOS and Linux, and the editor bundles them. Export can target any OS from any OS. Every export includes a generated licenses and credits file with the engine's `NOTICE` and dependency licenses. Add the optional "Made with Axcenity" splash screen (on by default, with a project setting to turn it off). Document in the README that games belong to their creators and how forks must handle the `NOTICE` and the name.

### Acceptance criteria

- [ ] From Windows, export a game that runs on macOS and Linux, and vice versa (tested manually on real machines or VMs).
- [x] Every export contains the credits file with the `NOTICE` text.
- [x] The splash shows by default, and the project setting turns it off.
- [x] The README covers game ownership, the fork rules and how to open unsigned macOS apps.

---

# Ongoing

## Phase 29: Audio

**User stories**: 48

### What to build

Create the `audio` crate around `kira`. Add a `Sound` Instance (asset reference, `Volume`, `Looped`, `Play`/`Stop`) usable from Lua and blocks. Sounds stop when Play stops in the editor.

### Acceptance criteria

- [x] A script plays a sound effect when a part is touched, and background music loops.
- [x] Stopping Play silences all game audio.
- [x] Sound blocks exist and translate to the same API.

---

## Phase 30: Render stage 2, shadows

**User stories**: 40, 41

### What to build

Add sun shadow maps (cascaded if needed for large scenes), configurable from the lighting service. (Since built: three cascades, contact-hardening soft shadows with `Lighting.ShadowSoftness`, and `CastShadow` on parts.)

### Acceptance criteria

- [x] Parts cast and receive sun shadows in the editor and the player.
- [ ] All earlier example projects still render correctly.
- [x] Shadows can be turned off for low-end machines.

---

## Phase 31: Render stage 3, point and spot lights

**User stories**: 11, 40, 41

### What to build

Add `PointLight` and `SpotLight` Instances (parented to parts) with color, brightness, range and angle. The renderer supports many lights efficiently (for example, clustered or tiled lighting).

### Acceptance criteria

- [ ] Lights can be inserted, edited and scripted, and changes show live.
- [ ] A scene with 100+ lights stays interactive on a mid-range GPU.
- [ ] Earlier example projects are unaffected.

---

## Phase 32: Render stage 4, sky and environment lighting

**User stories**: 40, 41

### What to build

Add a configurable sky (a procedural sky and/or HDR skybox) and image-based environment lighting, so metallic and glossy materials reflect their surroundings.

### Acceptance criteria

- [x] Metal parts visibly reflect the sky.
- [x] Sky settings are editable in the lighting service and save with the project.
- [ ] Earlier example projects are unaffected.

---

## Phase 33: Render stage 5, post-processing

**User stories**: 40, 41

### What to build

Render in HDR, then apply tone mapping and bloom, configurable per project. Neon materials bloom.

### Acceptance criteria

- [ ] Neon parts glow with bloom, and bright scenes don't clip harshly.
- [x] Post-processing can be adjusted or turned off per project.
- [ ] All example projects render correctly with the final pipeline on all three OSes.
