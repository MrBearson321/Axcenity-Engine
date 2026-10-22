# Plan: Axcenity Engine v3, editor workspace

> No PRD. The source is the developer's request of 2026-10-22 and the design answers below. v1 and v2 rules still hold. The phases run in order, and each one is committed when it's done.

## Design decisions (chosen by the developer)

- **Documents are dock tabs.** Every open script, stylesheet and block script is its own top-level dock tab beside Scene View, with no tab strip inside Scene View. You can drag it out, split it beside the viewport, or float it. The Animation Studio splits into an **Animation** tab (clips) and an **Animator** tab (controllers). The **UI Designer** is a tab too.
- **Console replaces Output.** It shows the same log, plus a command line. Lua runs against the edit world while editing (each command is one undo step) and against the running game during Play. There are built-in commands (`help`, `clear`, `select`, …), history and autocomplete.
- **Panels work at any shape.** Every panel stays usable when it's docked tall and narrow: no horizontal overflow, and toolbars wrap or collapse.
- **Animation has its own preview.** Posing and previewing happen in a viewport inside the Animation tab, isolated from the edited world, not in the Scene View. You can keyframe skeletal rigs (bones), and Parts, Models and UI elements (properties). Keys have easing and there is a curve editor. The layout is simpler.
- **The UI Designer is an artboard, and its node graph handles logic.** The selected ScreenGui is shown on a design-resolution artboard for layout and styling, and it edits the same Instances and CSS. Each ScreenGui has one Blueprint-style **Event Graph** for logic. The graph compiles to a generated LocalScript (one Lua runtime). The designer's preview runs the graph live without Play. Node graphs are for UI only for now, but the graph core is generic.

## Architectural decisions

- `player` still never depends on `editor`, `egui` or `blocks`. The graph-to-Lua compiler lives next to the blocks translator, so the generated Lua is ordinary Lua for the player.
- Console Lua uses the same sandbox as scripts. In edit mode it runs in a throwaway runtime bound to the edit world, and its changes are recorded as one history step.
- New on-disk files are optional (`*.uigraph.json`, pose clips), so they don't bump the project format version. A changed clip file shape adds a migration.

---

## Phase 1: Documents as dock tabs

### What to build

Script, stylesheet and block-script tabs become `egui_dock` tabs (`Pane::Script`) instead of a strip inside Scene View. Opening a script focuses its tab. A new tab opens next to the last script tab you used, or beside Scene View. Closing a tab asks about unsaved changes as before. The tab title shows the class icon, the name, an unsaved dot and the side warning. Code/Blocks switching and stylesheet live restyling work in any tab, and several scripts can be visible at once. Script tabs are not saved in the layout between runs.

### Acceptance criteria

- [ ] Opening a script, LocalScript, ModuleScript, stylesheet or block script shows it as its own dock tab beside Scene View, and opening one that's already open focuses it.
- [ ] Script tabs can be dragged beside the viewport, split and floated, and the viewport keeps working beside them.
- [ ] Closing a tab with unsaved changes asks first, and closing the project closes every script tab.
- [ ] Jumping from an error in Output to `path:line` focuses that tab at that line.
- [ ] Renaming a script keeps its tab where it is.

## Phase 2: Console

### What to build

Output becomes the **Console**: the same severity-filtered log with clickable `path:line`, plus a command line at the bottom. Lua typed there runs in edit mode against the edit world (one undo step, and errors go to the log) or during Play against the session's runtime. Built-ins: `help`, `clear`, `select <name|path>`, `find <name>`, `open <script>`, `play`, `stop`, `save`. You get Up/Down history (saved between runs), Tab autocomplete for built-ins, globals and Instance paths, multi-line input with Shift+Enter, and echoed commands with their results.

### Acceptance criteria

- [x] `print(workspace.Baseplate.Size)` prints the value while editing and during Play.
- [x] `workspace.Baseplate.Transparency = 0.5` while editing changes the part, and one Undo reverts it.
- [x] Built-ins work, and unknown commands and Lua errors are reported, not crashed on.
- [ ] Up/Down walk the history, and Tab completes.

## Phase 3: Responsive panels

### What to build

Every panel adapts to its width and height. In a narrow column, Assets switches from tiles to a list, and its chips and search wrap onto their own rows. The Explorer, Properties (the name column shrinks, then values stack under names), Console, Terrain Studio, Animation, Animator and UI Designer keep their content inside the panel. There is a shared helper for "toolbar that wraps" and "two-pane layout that stacks when narrow". Drawing tests render each panel at 260×900 and 900×260.

### Acceptance criteria

- [ ] Each panel docked into a 260-point-wide column has no horizontal overflow, and every control is reachable.
- [ ] Assets in a narrow column shows a readable list, and in a wide panel the tiles.
- [ ] Snapshot tests at tall and wide sizes pass.

## Phase 4: Animation and Animator tabs, simpler layout

### What to build

Split the Animation Studio into two dock tabs. The **Animation** tab is for clips, laid out top to bottom:
- a toolbar (clip picker, New, model picker, Play/Loop/Speed, Record)
- the preview, with the bone/object list as a collapsible side drawer
- a dope sheet / timeline docked at the bottom
- the settings in a collapsible inspector

The **Animator** tab is for controllers, with the graph, inspector and live preview. Each tab has its own document and preview. The step bar is replaced by an empty-state checklist. The Library becomes a searchable picker.

### Acceptance criteria

- [ ] A clip and a controller can be open at once, each in its own tab.
- [ ] Everything the old panel did (clip ranges, events, controllers, blend trees, Return when) still works.
- [ ] The Animation tab is usable at 600×400 and stacks when narrow.

## Phase 5: Keyframing Parts, Models and UI

### What to build

Property clips get tracks for Position, Rotation, Size, Color and Transparency of any Part, a Model's pivot, and its descendant Parts by path, and for UI element Position, Size, BackgroundTransparency and similar. The preview shows a copy of the chosen Model or ScreenGui. Select an object in the drawer or the preview, then:
- move it with gizmos in the preview, and with Record on, a key is set at the playhead
- or use "Key selected" (K)

The dope sheet supports box select, move, copy/paste, delete and snap to frames. Playback in the game follows the Animation instance as today.

### Acceptance criteria

- [ ] A door Model can be animated by rotating its hinge part in the preview with Record on, and it plays the same in Play.
- [ ] Older Position-only property clips still open and play.

## Phase 6: Posing skeletal rigs

### What to build

In the Animation tab, a skinned model's bones show as a tree in the drawer and as pickable joints in the preview. Rotate a bone, or move the root, with gizmos, and key it (Record, or K). A new pose-clip kind stores per-bone rotation and position tracks. Animators play it like take clips, so it works in controllers and blend trees. An existing take clip can be "Copied to keyframes" so it can be edited.

### Acceptance criteria

- [ ] Making a 3-key wave on an imported rig plays in the preview, in a controller state, and in Play.
- [ ] A take clip copied to keyframes looks the same, then can be edited.

## Phase 7: Easing and curve editor

### What to build

Every key has an easing: Constant, Linear, Ease In/Out/InOut × Sine, Quad, Cubic, Back, Bounce or Elastic, or Bezier with tangent handles. A Curves view toggles with the dope sheet. It shows the selected tracks' value curves with zoom and pan, draggable keys and tangent handles, and frame-all. Both property clips and pose clips use it. The `anim` crate evaluates the same curves.

### Acceptance criteria

- [ ] Changing a key's easing changes the motion in the preview and in Play.
- [ ] Bezier tangents dragged in the curve editor are saved and reloaded.

## Phase 8: UI Designer artboard

### What to build

The **UI Designer** dock tab shows the selected ScreenGui on an artboard at its design size (DesignWidth×DesignHeight, with presets for 1920×1080, 1280×720, phone portrait and landscape, and custom sizes), with zoom and pan. It has a palette of UI elements to drag on, a hierarchy, selection handles with snapping (shared with `ui_handles.rs`), and the Style (CSS) inspector. Edits go through `EditorApp::change` and the stylesheet tabs, so the Scene View and Play match.

### Acceptance criteria

- [ ] A menu with a Frame, a TextLabel and two Buttons can be built and styled in the designer only, and it looks the same in Play.
- [ ] Every designer edit is undoable.

## Phase 9: UI Event Graph

### What to build

A generic node-graph editor (nodes, typed pins, wires, comments, pan/zoom, box select, copy/paste, a searchable node menu) and a UI node library:
- **events**: Button Activated, Hovered, TextBox FocusLost, Screen Shown, Every Frame
- **actions**: Set Property, Show Page, Set Visible, Tween, Play Sound, Fire RemoteEvent, Print
- **flow**: Branch, Sequence, Delay
- **data**: Get Property, Get Text, math, compare, string join, variables

Each ScreenGui has a graph saved as `<ScreenGui>.uigraph.json`, which compiles to a generated LocalScript under the ScreenGui (read-only, regenerated on save). An Event Graph sub-tab sits in the UI Designer. The designer's Live Preview runs the compiled Lua in a sandboxed preview world, so clicking a button in the designer runs its graph.

### Acceptance criteria

- [ ] "Button Activated → Show Page Settings" works in Live Preview and in Play.
- [ ] Bad graphs (unconnected required pins) show errors on the nodes, not Lua errors.
- [ ] The generated LocalScript is ordinary Lua the player runs without the editor.
