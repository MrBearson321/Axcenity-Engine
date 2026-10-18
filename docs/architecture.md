# Architecture

Read before creating a crate, adding a dependency between crates, or touching the frame loop.

## Crate map (Cargo workspace)

| Crate | Owns | May depend on |
|---|---|---|
| `core` | Instance tree, arena, properties, events, services | nothing engine-internal |
| `render` | `wgpu` renderer | nothing engine-internal; takes a draw list |
| `scene` | Instance tree → draw list (`build_draw_list`), the shared `FlyCamera`, viewport picking (`pick`) | `core`, `render` |
| `physics` | `rapier3d` integration | `core` |
| `script` | Lua 5.4 (`mlua`), sandbox, scheduler, engine API | `core` |
| `blocks` | block-script model and translation to Lua | `core` (not `script`) |
| `ui` | game UI: CSS subset, `taffy` layout, UI draw data | `core` |
| `audio` | `kira` integration | `core` |
| `project` | project folder load and save, export packing | `core` |
| `player` | the runtime shipped with games (binary) | `core`, `render`, `scene`, `physics`, `script`, `ui`, `audio`, `project` |
| `editor` | `egui` editor app (binary) | everything |

Rules:
- `player` must **never** depend on `editor`, `egui` or `blocks`. The editor runs the export pipeline: `blocks` translates block scripts to Lua, then `project` packs the result.
- `render` never sees Instances, scripts or the editor. Something outside it turns the world into a draw list (meshes, materials, transforms, lights, camera).
- Don't let crates depend on each other in a cycle. If two crates need each other, move the shared type into `core`.
- Pin dependency versions in the workspace's root `Cargo.toml` (`[workspace.dependencies]`).

## Data model

- One tree of **Instances**: class, `Name`, `Parent`, children, typed properties and events.
- Instances live in an **arena** and are referenced by **generational IDs** (index plus generation). Never use `Rc<RefCell<…>>` parent/child pointers.
- An ID can outlive its Instance. Every lookup returns an `Option` or `Result`; never assume the Instance still exists.

## Frame order (fixed, do not reorder casually)

1. Read input
2. Resume coroutines whose wait has expired, and fire queued events
3. Step physics at a fixed 60 Hz (accumulator; may run 0–N steps per frame)
4. Fire `Touched` and collision events
5. Update UI layout (`taffy`)
6. Build the draw list and render

The player runs these in `WindowEvent::RedrawRequested` (`crates/player/src/main.rs`), with a comment per step. Step 5 arrives with its phase. The editor runs step 6 every frame it draws, and steps 1-4 only during Play (`Session::frame` in `crates/editor/src/play.rs`), before any panel is drawn.

Since Phase 14, step 1 also hands LocalScripts the camera (`set_camera`) and the key presses since the last frame (`ScriptRuntime::input`), and step 2 first delivers RemoteEvent messages to each side (server, then client) before resuming its tasks. Where a LocalScript moved the camera (`take_camera_move`) is applied just before step 6. See `docs/scripting.md` (Server and client).

## Physics (decided in Phase 6)

- `Anchored` parts are fixed colliders; unanchored parts are dynamic bodies. New parts default to `Anchored = false`, like Roblox.
- Gravity is a constant 196.2 studs/s² (Roblox's default) until a `Workspace.Gravity` property exists.
- Fixed 60 Hz steps from an accumulator, at most 10 per frame. The same steps give the same results at any frame rate. There's no interpolation yet, so above 60 fps moving parts update in 60 Hz steps.
- `physics` finds script changes by comparing each Part with what it last read or wrote, not through events. Setting `Position` or `Orientation` teleports a part **and stops it** (decided in Phase 6; Roblox keeps the speed). Toggling `Anchored` also starts the part from rest.
- **Touched** (decided in Phase 7): after every fixed step, `physics` compares the pairs of parts with a solver contact against the pairs touching before, and queues the new ones (`Physics::take_touches`). Frame step 4 fires `Touched` on both parts of each pair. Contacts already there in the very first step (parts resting on each other when the game starts) aren't reported. A pair stops counting as touching only after 3 steps in a row apart, so resting parts that lose contact for a step don't fire again, while real bounces do. Two anchored parts never touch (rapier doesn't test fixed against fixed), as in Roblox.
- One body per Part, no welds or joints yet. Parts with NaN or infinite numbers are left out. Friction 0.5 and bounciness 0.25 for every material, plus a little angular damping because rapier has no rolling resistance.

## Editor (decided in Phase 8)

- Built on **eframe 0.32** (egui's app shell) with its wgpu backend. eframe owns the window, event loop and GPU device; it must stay on the same wgpu major as `render` (0.32 ↔ wgpu 25, winit 0.30), so upgrade them together.
- Layout like Roblox Studio: menu bar on top, **Explorer** down the right side, **Output** along the bottom under the viewport, viewport in the middle.
- **Viewport**: `render::TextureRenderer` draws into a texture sized in physical pixels, registered with egui as a native texture and re-pointed after each resize. Only redraws when egui does (input or a requested repaint), so an idle editor uses no GPU.
- **Camera** (Studio-style): right-drag looks (cursor hidden; `Locked` on macOS, `Confined` elsewhere), WASD/E/Q fly while the mouse is over the viewport, Shift slows, wheel zooms toward the focus point, middle-drag pans, Alt+left-drag orbits the focus point. Keys are read by physical position, like the player.
- **Fly speed** (added to Phase 8 on request): Shift+wheel, or the wheel while right-dragging (like Unity and Godot), changes the WASD speed by 20% per notch, from 1 to 500 studs/s (default 20). A pop-up in the middle of the viewport shows the new speed for a second, then fades. The speed survives opening another project but isn't saved between sessions. The player keeps a fixed 20 studs/s.
- **Explorer**: shows the real root (`Game`, class `DataModel`; `game` in scripts) and every Instance under it. `game` and `Workspace` start open, everything else closed. The class is shown next to names that differ from it.
- **Output**: editor messages (project opened, with object and script counts) and load errors, one line each with the full cause chain (file and line for bad JSON). A failed open keeps the current project. During Play, script output and errors join it.
- **Opening projects**: `axcenity-editor <folder>` or File > Open Project… (`rfd` native folder dialog; the desktop portal on Linux). With no folder, the editor starts empty with a hint in Output.
- Scripts and physics only run during Play (see Play mode).

### Selecting and editing (decided in Phase 9)

- **One selected Instance** (`EditorApp::selected`), shared by the Explorer, Properties and viewport. Multi-select can come later.
- **Viewport picking**: a left click (not a drag, not Alt) casts a ray from the camera (`scene::CameraView::ray`) and picks the nearest Workspace Part whose *drawn* shape it hits (`scene::pick`: exact box, wedge slope, sphere and cylinder tests, using the same transform as the draw list). Parts the camera is inside are skipped. Clicking empty space clears the selection. Picking in the viewport opens the Explorer branches above the part and scrolls to it.
- **Highlight**: the selected part gets a blue (25, 160, 255) wireframe box around its drawn mesh, painted by egui on top of the viewport image, so it shows through other parts. Like Studio's selection box, balls and cylinders get a box too. `render` is untouched. In the Explorer, the selected row is highlighted.
- **Properties panel**: under the Explorer, in collapsible groups like Studio: Data (Name, ClassName), Appearance (Color, Material, Metalness, Roughness), Behavior (Anchored), Part (Shape, Position, Orientation, Size). A property no group lists goes under "Other", so new properties always show. Controls: checkbox for bools, dropdown for enums, a color picker (egui's, converted from sRGB to linear light and back) with the 0–255 values, and drag-number boxes (drag to scrub, click to type) for numbers and each vector axis. Metalness and Roughness stay within 0–1, and Size within 0.001 and up (Roblox's minimum); values already out of range are never clamped just by being shown. Numbers show up to 3 decimals.
- **Edits apply live**: the panel returns an `Edit`, and the app applies it to the world before the viewport draws, in the same frame. Setting a value it already has doesn't count as a change. Service names, and Script names (which come from their files), are read-only.
- **Saving**: File > Save or Ctrl+S (Cmd+S on macOS) writes `world.json` with `project::save_world`; scripts and the manifest aren't touched. Unsaved changes put `*` after the project name in the title and "Unsaved changes" at the right of the menu bar. Closing the window, File > Quit or File > Open Project… with unsaved changes asks "Save changes?" (Save / Don't Save / Cancel; Escape or clicking outside is Cancel). If saving fails, the error goes to Output and the editor stays open.

### Transform tools and undo (decided in Phase 10)

- **Tools**: a toolbar under the menu bar with Select, Move, Scale and Rotate, also on Ctrl+1–4 (Cmd on macOS), as in Studio. Select is the default. Every tool still selects on a plain click; a click that grabs a handle doesn't.
- **Handles** (`crates/editor/src/gizmo.rs`), painted by egui over the viewport like the selection box, a fixed size on screen (90 points), red/green/blue for X/Y/Z, yellow when hovered or held:
  - Move: an arrow per **world** axis from the part's center. Arrows pointing almost straight at the camera are hidden.
  - Scale: a ball on each of the 6 faces, along the part's **own** axes. Dragging one moves that face, and the opposite face stays put (Position moves by half the change). Size stops at 0.001.
  - Rotate: a ring around each **world** axis through the center. The angle counts past ±180°, so you can keep turning.
  - No local/world toggle yet (Studio's Ctrl+L); it can come later.
- **Snapping**: toolbar "Snap move" (studs, also used by Scale) and "Snap rotate" (degrees), each with its own checkbox and step, both on at 1 stud and 15° by default. Steps count from where the drag started, so a part off the grid stays off it. Rotations are tidied to 3 decimals so 15° steps store as 15, not 14.999999. Tool and snap settings are preferences: they survive opening another project but aren't saved between sessions (like the fly speed).
- **Drag to place** (added to Phase 10 on request; `crates/editor/src/placing.rs`): with any tool, left-dragging a part by its body (not a handle) moves it like Studio. It starts only after egui's drag threshold, so a click still just selects, and grabbing an unselected part selects it. Each frame a ray from the mouse finds the nearest surface, skipping the dragged part (`scene::pick_hit`, which reports the surface normal). The part rests against that surface without turning: its center is lifted off the surface by its drawn box's reach along the normal (`scene::part_half_extent`), so it sits on floors, stacks on parts and goes beside walls. The grab point stays under the mouse sideways along the surface. Over empty sky it stays put. With Snap move on, the two coordinates along a flat or upright surface snap to the **world** grid (unlike the handles, which snap relative to the start); slopes and balls don't snap. One drag is one "Move" undo step. Balls and cylinders rest by their box, as in Studio.
- **Undo** (`crates/editor/src/history.rs`): every change goes through `EditorApp::change`, which snapshots the Instance's name and properties before and after and records the differences as one step. Side effects are recorded too (a new Material's Metalness and Roughness), and undo restores Material first so they come back right. Changes merge into the open step while they edit the same field of the same Instance, until the gesture ends: the mouse is let go (a handle, a number drag, the color picker) and no text box has focus (typing a name or a number). So one drag or one typed name is one step. A drag that ends where it started leaves no step.
- **Shortcuts**: Edit > Undo/Redo name the step ("Undo Move Pad"). Ctrl+Z undoes; Ctrl+Y or Ctrl+Shift+Z redo (Cmd on macOS). While a text box has focus, Ctrl+Z goes to the text box instead. Undo is ignored mid-drag.
- **Saving keeps the history.** It only remembers which step was on top, so undoing back to that point counts as saved (no `*`), and undoing past it is unsaved again. The history isn't written to disk, and opening a project starts a new one.

### Tree editing (decided in Phase 11)

- **Model and Folder** classes group Instances. Neither has properties yet (no `PrimaryPart` or pivot), and clicking a part in the viewport still selects that part, not its Model. Scripts can make them with `Instance.new("Model")`.
- **Inserting**: toolbar buttons Block, Sphere, Cylinder and Wedge (chosen by the developer), and a right-click menu on Explorer rows with Insert ▸ (the four parts, Model, Folder). New things go **into the selected Model or Folder** if one is selected (or right-clicked), otherwise into the Workspace, and become the selection. Parts go at the camera's focus point, on the move grid when Snap move is on (otherwise rounded to 3 decimals), with the core defaults (unanchored, plastic). Sizes: blocks and wedges 4×1×2, spheres 2×2×2, cylinders 4×2×2 (4 long along X, so they read as cylinders). Every primitive is named `Part`, as in Studio.
- **Unique names** (`crates/editor/src/tree.rs`): a new or duplicated Instance gets the first of `Name`, `Name2`, `Name3`… that none of its siblings has. A number already on the end is replaced, so a copy of `Pad2` is `Pad3`. Renaming and moving don't enforce this.
- **Rename** with F2, Edit > Rename or the right-click menu: the row turns into a text box; Enter or clicking away keeps it, Escape cancels, an empty name cancels. One undo step.
- **Duplicate** (Ctrl+D / Cmd+D) copies the whole subtree into the same parent, right after the original, in the same place in the world, and selects the copy. **Delete** (Delete, or Backspace on macOS) removes the selection and everything in it.
- **Reparent** by dragging an Explorer row onto another row: it becomes that row's last child. A valid target is outlined in blue; an invalid one shows a "not allowed" cursor and the reason. Rejected: into itself or its own descendants, and anywhere outside the Workspace (the world file only saves the Workspace). There's no reordering among siblings yet.
- **Locked**: services can't be deleted, duplicated, moved or renamed. Scripts can be renamed and deleted since Phase 13 (see Script editing), but not duplicated or moved. Menu items are greyed out with the reason on hover.
- **Undo** (`history.rs`): tree changes are `Tree` steps holding `Move`s (an Instance's parent and child index before and after; `None` is out of the tree). **Deleting only detaches**: the Instance keeps its ID, properties and children outside the tree, so undo puts the very same Instances back at the same index, and older steps naming them still work. When an undone insert or duplicate can no longer be redone (a new change clears redo), its detached Instances are destroyed. After undo, redo or delete, a selection that left the tree is cleared. Tree edits are ignored while a part is being dragged in the viewport.

### Script editing (decided in Phase 13)

- **Tabs** (chosen by the developer): a strip above the viewport, like Studio: **World** (the viewport) first, then one tab per open script. A script tab replaces the viewport while shown; Explorer, Properties and Output stay. Double-click a Script in the Explorer (or right-click > Open) to open it. Tabs close with × or a middle click; a tab with unsaved changes asks "Save changes?" (Save saves only that script). Play switches to World. Tabs are keyed by file path (`crates/editor/src/scripts.rs`).
- **Code editor**: egui's `TextEdit` in code mode, monospace, line numbers in a gutter, long lines scroll sideways instead of wrapping (so the numbers line up), Tab indents. Lua highlighting is a small hand-written tokenizer (`lua_syntax.rs`, no new dependency): keywords, `true`/`false`/`nil`, engine and standard globals, numbers, strings (including `[[long]]` ones) and comments, in VS Code-like colors for the dark and light themes. Script tabs stay editable during Play; changes run at the next Play.
- **Insert Script** (toolbar **Script** button, or Insert ▸ Script on any Explorer row) makes `scripts/Script.server.lua` (or `Script2`… if taken, including by a file that only differs in capitals) containing `print("Hello world!")`, plus its Script in ServerScriptService, then selects and opens it. It's a file operation, so it isn't an undo step.
- **Saving** (chosen by the developer): one Save (Ctrl+S, File > Save, the "Save changes?" prompt, Play's autosave) writes `world.json` and every script tab with unsaved changes. Unsaved tabs show `●` and count toward the title's `*`. `world.json` is written when the world changed, or on a plain Save with nothing else to save (as before), but never over a world file the editor didn't load (see Damaged files) unless the world changed.
- **Rename and delete** (chosen by the developer): F2 / Edit > Rename in the Explorer renames the `.server.lua` file in its folder, all at once on Enter. The Properties Name box stays read-only for Scripts, so the file isn't renamed at every key. Names must be valid file names on every OS (Windows rules: none of `/ \ : * ? " < > |`, no `CON`, `COM1`…, no trailing dot or space). Delete asks "Delete X? This deletes the file. It can't be undone." Neither is an undo step; Scripts are never in the undo history.
- **Order**: Scripts in ServerScriptService are kept in file path order (the order they start in), also after inserting or renaming.
- **Watching the folder** (chosen by the developer: polling, not the `notify` crate): `watcher.rs` runs a background thread that checks `world.json` and every `scripts/**/*.server.lua` every 0.5 s by modification time and size, sends the changed paths, and wakes egui only when something changed, so an idle editor stays idle. The editor reads each changed file and compares its text with what it last read or wrote, so its own saves are ignored. A changed script updates its tab; if the tab has unsaved changes, it keeps them and shows a bar with **Reload from disk** (saving replaces the file). A new file adds a Script; a deleted one removes it (its tab closes, or stays with "deleted; Save to write it back" if it had changes). A changed `world.json` is reloaded (new undo history, selection cleared) unless the world has unsaved changes; then the editor keeps its world, and the other file is kept as `world.backup.json` on the next save. During Play, changes wait until Stop.
- **Damaged files**: opening reports each file that can't be loaded in Output ("Skipped a damaged file: …", with the line for bad JSON) and loads the rest. A damaged `world.json` gives an empty Workspace with a hint; fixing the file elsewhere loads it. Saving over a world file the editor never loaded first renames it to `world.backup.json` (`world.backup2.json`… if taken), so nothing is lost.
- **Error links**: Output lines of kind Error that start with `<path>.lua:<line>:` underline that part, and clicking opens the script at that line (cursor placed, scrolled into view, the line highlighted until the code is edited).

### Server and client (decided in Phase 14)

- **Explorer**: `game` has Workspace, Players, ReplicatedStorage, ServerScriptService and StarterPlayer (with StarterPlayerScripts), in that order, all locked like services. During Play, Players holds the Player.
- **LocalScripts**: a toolbar **LocalScript** button next to **Script**, and Insert ▸ LocalScript, make `scripts/LocalScript.client.lua` and its LocalScript in StarterPlayerScripts. Opening, renaming (keeps `.client.lua`), deleting, watching and Play's autosave work as for Scripts. Scripts and LocalScripts each stay in path order in their service.
- **RemoteEvents**: Insert ▸ RemoteEvent (Explorer menu only, not the toolbar). It goes into the selected Model or Folder, or ReplicatedStorage itself if that's selected, otherwise into ReplicatedStorage. **ReplicatedStorage is saved** like the Workspace, so anything can be inserted, moved (dragging rows) or edited there; `tree::is_saved` says where.
- **Warning badges**: `side_warnings.rs` scans each script's code (its tab's text if open, so it updates while typing) for side-only API names and strings from the script crate's `SIDE_ONLY_APIS`, skipping comments. A script with a hit gets a ⚠ after its Explorer row (hover lists `Line N: message`); a closed branch holding one shows ⚠ too, so it's never hidden. Its tab title gets ⚠, and the tab lists the warnings above the code; clicking one goes to the line. It's a text check: a variable that happens to be called `LocalPlayer` in a Script is warned about too. Warnings are cached by path and only worked out again when the text changes.
- **Play input**: during Play, keyboard events go to LocalScripts (`play::GameKeys`) unless a text box has the keyboard; then, or when the window loses focus, held keys are let go. Keys use their physical place when egui knows it. Shift, Ctrl and Alt come from egui's modifiers, so they always arrive as the left one. WASD still flies the editor camera too, as in the player.
- **Play camera**: LocalScripts see the viewport camera as `workspace.CurrentCamera`, and setting it moves the viewport camera (`Viewport::camera_view` / `set_camera_view`).

## Play mode

Play runs in-process on a **deep copy** of the edit world, and Stop drops the copy. Before Play: autosave. During Play: the Lua watchdog is active (Phase 15), and Rust panics are caught at the Play boundary and reported to the Output panel.

### Decided in Phase 12

- **Controls**: **▶ Play** and **■ Stop** at the left of the toolbar, only one enabled at a time, and **F5** / **Shift+F5** as in Studio (chosen by the developer). A green "Playing" label shows at the end of the toolbar.
- **The copy**: `World` derives `Clone`. The arena has no pointers, so a clone is a full deep copy where every Instance keeps its `InstanceId`. The selection and open Explorer branches carry over to the Play world and back. A fresh `ScriptRuntime` and `Physics` are made for each Play, so nothing (connections, waiting threads, bodies) survives into the next one. After Stop, a selection that only existed in the game is cleared.
- **Autosave** (chosen by the developer): Play saves `world.json` first **only if there are unsaved changes**; otherwise the file already matches, and Play doesn't rewrite it. If saving fails, Play doesn't start and Output says why. The undo history is kept across Play.
- **Scripts** are read again from their files at Play (`project::read_script`), so Play runs exactly what's on disk (Play's autosave writes unsaved script tabs first). A file that can't be read is reported and left out; the rest still run.
- **Read-only during Play** (chosen by the developer): the Explorer, Properties and viewport show the live game world. You can select and inspect (Properties values are greyed out but update every frame), open and close Explorer branches, and move the camera. Editing, the transform tools, dragging, inserting, the right-click menu, rename, undo and redo are all off. File > Save still saves the edit world. Opening another project stops the game.
- **Frame**: each editor frame during Play runs frame steps 2-4 with the real time since the last one (capped at 0.1 s, like the player), then draws; the editor repaints continuously while playing. The editor camera keeps working (there are no characters yet).
- **Output**: `print` lines are Info, script errors (`path:line: message`) are Error.
- **Panics** (`catch_unwind` around `Session::start` and `Session::frame`): the session is thrown away, Output shows "The engine crashed during Play: <message>...", and the editor is back in edit mode. The panic's location goes to the terminal (Rust's default panic message). Panics while *drawing* the Play world aren't caught yet. In debug builds, **Debug > Panic during Play** makes the next frame panic, to test this (chosen by the developer); release builds have no Debug menu.

## Portability

Target Windows, macOS and Linux. All runtime file access goes through the asset system, never direct `std::fs` calls in gameplay code, so web and mobile stay possible later.
