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

The player runs these in `WindowEvent::RedrawRequested` (`crates/player/src/main.rs`), with a comment per step. Step 5 arrives with its phase. The editor only runs step 6 until Play exists.

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
- **Output**: editor messages (project opened, with object and script counts) and load errors, one line each with the full cause chain (file and line for bad JSON). A failed open keeps the current project. Script output joins it with Play.
- **Opening projects**: `axcenity-editor <folder>` or File > Open Project… (`rfd` native folder dialog; the desktop portal on Linux). With no folder, the editor starts empty with a hint in Output.
- Scripts and physics don't run in the editor until Play (Phase 12).

### Selecting and editing (decided in Phase 9)

- **One selected Instance** (`EditorApp::selected`), shared by the Explorer, Properties and viewport. Multi-select can come later.
- **Viewport picking**: a left click (not a drag, not Alt) casts a ray from the camera (`scene::CameraView::ray`) and picks the nearest Workspace Part whose *drawn* shape it hits (`scene::pick`: exact box, wedge slope, sphere and cylinder tests, using the same transform as the draw list). Parts the camera is inside are skipped. Clicking empty space clears the selection. Picking in the viewport opens the Explorer branches above the part and scrolls to it.
- **Highlight**: the selected part gets a blue (25, 160, 255) wireframe box around its drawn mesh, painted by egui on top of the viewport image, so it shows through other parts. Like Studio's selection box, balls and cylinders get a box too. `render` is untouched. In the Explorer, the selected row is highlighted.
- **Properties panel**: under the Explorer, in collapsible groups like Studio: Data (Name, ClassName), Appearance (Color, Material, Metalness, Roughness), Behavior (Anchored), Part (Shape, Position, Orientation, Size). A property no group lists goes under "Other", so new properties always show. Controls: checkbox for bools, dropdown for enums, a color picker (egui's, converted from sRGB to linear light and back) with the 0–255 values, and drag-number boxes (drag to scrub, click to type) for numbers and each vector axis. Metalness and Roughness stay within 0–1, and Size within 0.001 and up (Roblox's minimum); values already out of range are never clamped just by being shown. Numbers show up to 3 decimals.
- **Edits apply live**: the panel returns an `Edit`, and the app applies it to the world before the viewport draws, in the same frame. Setting a value it already has doesn't count as a change. Service names, and Script names (which come from their files), are read-only.
- **Saving**: File > Save or Ctrl+S (Cmd+S on macOS) writes `world.json` with `project::save_world`; scripts and the manifest aren't touched. Unsaved changes put `*` after the project name in the title and "Unsaved changes" at the right of the menu bar. Closing the window, File > Quit or File > Open Project… with unsaved changes asks "Save changes?" (Save / Don't Save / Cancel; Escape or clicking outside is Cancel). If saving fails, the error goes to Output and the editor stays open.

## Play mode

Play runs in-process on a **deep copy** of the edit world, and Stop drops the copy. Before Play: autosave. During Play: the Lua watchdog is active, and Rust panics are caught at the Play boundary and reported to the Output panel.

## Portability

Target Windows, macOS and Linux. All runtime file access goes through the asset system, never direct `std::fs` calls in gameplay code, so web and mobile stay possible later.
