# Plan: Axcenity Engine v3, editor workspace

> No PRD. The source is the developer's requests of 2026-10-22 and 2026-10-23 (animation and import redesign) and the design answers below. v1 and v2 rules still hold. The phases run in order, and each one is committed when it's done.

## Design decisions (chosen by the developer)

- **Documents are dock tabs.** Every open script, stylesheet and block script is its own top-level dock tab beside Scene View, with no tab strip inside Scene View. You can drag it out, split it beside the viewport, or float it. The Animation Studio split into an **Animation** tab (clips) and an **Animator** tab (controllers) in Phase 4; Phase 8 merges them back into one Animation tab (below). The **UI Designer** is a tab too.
- **Console replaces Output.** It shows the same log, plus a command line. Lua runs against the edit world while editing (each command is one undo step) and against the running game during Play. There are built-in commands (`help`, `clear`, `select`, …), history and autocomplete.
- **Panels work at any shape.** Every panel stays usable when it's docked tall and narrow: no horizontal overflow, and toolbars wrap or collapse.
- **Animation has its own preview.** Posing and previewing happen in a viewport inside the Animation tab, isolated from the edited world, not in the Scene View. You can keyframe skeletal rigs (bones), and Parts, Models and UI elements (properties). Keys have easing and there is a curve editor. The layout is simpler.
- **The UI Designer is an artboard, and its node graph handles logic.** The selected ScreenGui is shown on a design-resolution artboard for layout and styling, and it edits the same Instances and CSS. Each ScreenGui has one Blueprint-style **Event Graph** for logic. The graph compiles to a generated LocalScript (one Lua runtime). The designer's preview runs the graph live without Play. Node graphs are for UI only for now, but the graph core is generic.
- **Animation is one system** (2026-10-23):
  - **One clip type.** A clip (`*.anim.json`) holds tracks of any kind: bones, Part, Model and UI properties, and events. A clip made from an FBX take is **linked** to it ("Linked to noodle.fbx · Walk") and reads its bone motion from the file, so re-exporting from Blender updates it. **Convert to keys** bakes it into editable bone keys. While a clip is linked, the bones its take moves are locked (🔒); everything else can be keyed. Additive offset layers may come later.
  - **Two file kinds, named as the editor names them.** Clips are `*.anim.json`, and state machines (formerly "controllers") are `*.statemachine.json`. Each animated thing has its own folder, `assets/animations/<name>/`, holding its clips and its state machine. Shared clips go in `assets/animations/Shared/`.
  - **Roblox-familiar Instances.** An **Animator** plays animation on its parent, with an optional `StateMachine`. An **Animation** points at a clip (`Clip`) and has `AutoPlay`. `animator:LoadAnimation(animation)` returns an **AnimationTrack**.
  - **One Animation window, built around the object.** It has an outline of the Animator's state machine and Animations, a breadcrumb (Hero ▸ State Machine ▸ Walk), and one icon and color per kind everywhere. An **Animate** button sets up the Animator, the clip and the Animation in one step.
- **Importing and exporting are one flow** (2026-10-23): every way in opens one tabbed Import dialog (Objects · Materials · Animation) with a "What you'll get" summary above pinned buttons. **Export Package…** writes chosen assets and everything they need to one `.axpkg` file. Import lists clashes, with Keep both (the default), Replace or Skip existing. Export Game becomes **Build Game**.

## Architectural decisions

- `player` still never depends on `editor`, `egui` or `blocks`. The graph-to-Lua compiler lives next to the blocks translator, so the generated Lua is ordinary Lua for the player.
- Console Lua uses the same sandbox as scripts. In edit mode it runs in a throwaway runtime bound to the edit world, and its changes are recorded as one history step.
- New on-disk files are optional (`*.uigraph.json`, `*.axpkg`), so they don't bump the project format version. A changed clip file shape adds a migration.
- The one clip format and the rename to `*.statemachine.json` bump the project format version (`project::FORMAT_VERSION` 2 → 3). Opening an older project migrates it once: the files, the paths that point at them, and the Instances.
- `anim` samples every track kind (linked take, bone keys, property keys) through one clip type. Animators, state machines and AnimationTracks all play clips through it, and the player still has no editor dependency.
- `.axpkg` is a zip archive with a manifest. It needs one new workspace dependency for zip files, pinned, with its licence checked as Apache-2.0-compatible.

---

## Phase 1: Documents as dock tabs

### What to build

Script, stylesheet and block-script tabs become `egui_dock` tabs (`Pane::Script`) instead of a strip inside Scene View. Opening a script focuses its tab. A new tab opens next to the last script tab you used, or beside Scene View. Closing a tab asks about unsaved changes as before. The tab title shows the class icon, the name, an unsaved dot and the side warning. Code/Blocks switching and stylesheet live restyling work in any tab, and several scripts can be visible at once. Script tabs are not saved in the layout between runs.

### Acceptance criteria

- [x] Opening a script, LocalScript, ModuleScript, stylesheet or block script shows it as its own dock tab beside Scene View, and opening one that's already open focuses it.
- [x] Script tabs can be dragged beside the viewport, split and floated, and the viewport keeps working beside them.
- [x] Closing a tab with unsaved changes asks first, and closing the project closes every script tab.
- [x] Jumping from an error in Output to `path:line` focuses that tab at that line.
- [x] Renaming a script keeps its tab where it is.

## Phase 2: Console

### What to build

Output becomes the **Console**: the same severity-filtered log with clickable `path:line`, plus a command line at the bottom. Lua typed there runs in edit mode against the edit world (one undo step, and errors go to the log) or during Play against the session's runtime. Built-ins: `help`, `clear`, `select <name|path>`, `find <name>`, `open <script>`, `play`, `stop`, `save`. You get Up/Down history (saved between runs), Tab autocomplete for built-ins, globals and Instance paths, multi-line input with Shift+Enter, and echoed commands with their results.

### Acceptance criteria

- [x] `print(workspace.Baseplate.Size)` prints the value while editing and during Play.
- [x] `workspace.Baseplate.Transparency = 0.5` while editing changes the part, and one Undo reverts it.
- [x] Built-ins work, and unknown commands and Lua errors are reported, not crashed on.
- [x] Up/Down walk the history, and Tab completes.

## Phase 3: Responsive panels

### What to build

Every panel adapts to its width and height. In a narrow column, Assets switches from tiles to a list, and its chips and search wrap onto their own rows. The Explorer, Properties (the name column shrinks, then values stack under names), Console, Terrain Studio, Animation, Animator and UI Designer keep their content inside the panel. There is a shared helper for "toolbar that wraps" and "two-pane layout that stacks when narrow". Drawing tests render each panel at 260×900 and 900×260.

### Acceptance criteria

- [x] Each panel docked into a 260-point-wide column has no horizontal overflow, and every control is reachable.
- [x] Assets in a narrow column shows a readable list, and in a wide panel the tiles.
- [x] Snapshot tests at tall and wide sizes pass.

## Phase 4: Animation and Animator tabs, simpler layout

### What to build

Split the Animation Studio into two dock tabs. The **Animation** tab is for clips, laid out top to bottom:
- a toolbar (clip picker, New, model picker, Play/Loop/Speed, Record)
- the preview, with the bone/object list as a collapsible side drawer
- a dope sheet / timeline docked at the bottom
- the settings in a collapsible inspector

The **Animator** tab is for controllers, with the graph, inspector and live preview. Each tab has its own document and preview. The step bar is replaced by an empty-state checklist. The Library becomes a searchable picker.

### Acceptance criteria

- [x] A clip and a controller can be open at once, each in its own tab.
- [ ] Everything the old panel did (clip ranges, events, controllers, blend trees, Return when) still works.
- [x] The Animation tab is usable at 600×400 and stacks when narrow.

## Phase 5: Keyframing Parts, Models and UI

### What to build

Property clips get tracks for Position, Rotation, Size, Color and Transparency of any Part, a Model's pivot, and its descendant Parts by path, and for UI element Position, Size, BackgroundTransparency and similar. The preview shows a copy of the chosen Model or ScreenGui. Select an object in the drawer or the preview, then:
- move it with gizmos in the preview, and with Record on, a key is set at the playhead
- or use "Key selected" (K)

The dope sheet supports box select, move, copy/paste, delete and snap to frames. Playback in the game follows the Animation instance as today.

### Acceptance criteria

- [x] A door Model can be animated by rotating its hinge part in the preview with Record on, and it plays the same in Play.
- [x] Older Position-only property clips still open and play.

## Phase 6: Import dialog fits the window

### What to build

Fix the Import dialog so its buttons can always be reached. Today its sections are stacked with nothing scrolling the whole dialog: Objects, Materials, Bones and Clips come to about 900 points for an animated, textured FBX. On a smaller window, Import and Cancel end up below the screen. The fix:
- the dialog sizes itself to the window
- its sections scroll together in one body
- Import and Cancel are pinned at the bottom

Nothing else changes; Phase 9 redesigns the dialog.

### Acceptance criteria

- [ ] An animated FBX with texture maps can be imported in an 800×600 editor window, with Import visible and clickable without scrolling.
- [x] A drawing test opens the dialog with every section at 800×600, and Import is inside the window.

## Phase 7: One clip format, Animator, Animation and AnimationTrack

### What to build

**One clip type.** A clip (`*.anim.json`) has a duration, loop, events and tracks.
- A track names its target by path from the Animator's parent: `.` is the parent, `Hinge/Panel` is a descendant, and bones are named in the same scheme. A clip therefore plays on anything whose names match.
- A track animates a bone, or any property property clips key today.
- A clip can be **linked** to an FBX take (`source`, `take`, start and end, root motion). Its bones' motion is then read from the take, and its own tracks can only animate what the take doesn't move.
- Take clips and property clips both become this one shape.

**State machines.** Today's controllers become `*.statemachine.json`, with the same content. The Animator's `Controller` property becomes `StateMachine`, and "controller" becomes "state machine" in the UI, Lua errors and docs.

**Folders.** `assets/animations/<name>/` holds one animated thing's clips and its state machine. Shared clips go in `assets/animations/Shared/`. Importing an animated FBX and New clip write there.

**Instances and Lua:**
- **Animator** can go in any Part, MeshPart, Model, ScreenGui or UI element. It plays its `StateMachine`, if set, on its parent. Its state machine API stays as it is (`Play(state)`, `SetBool` and the rest, `CurrentState`, `StateChanged`, `AnimationEvent`), and it gains `GetPlayingAnimationTracks()`.
- **Animation** has `Clip` and `AutoPlay`. If `AutoPlay` is ticked, the Animator on the same parent plays it when Play starts.
- `animator:LoadAnimation(animation)` returns an **AnimationTrack** with:
  - methods `Play(fadeTime?, weight?, speed?)`, `Stop(fadeTime?)` and `AdjustSpeed(speed)`
  - properties `IsPlaying`, `Length`, `Looped`, `Speed` and `TimePosition`
  - the signal `Stopped` and `GetMarkerReachedSignal(name)` for events.
- Tracks blend over the state machine by weight. On something both animate, the most recently started track wins.

**Migration** (project format 2 → 3, once, when the project opens):
- Controllers are renamed and moved into their model's folder.
- Clips are rewritten to the one shape and moved by the same rule.
- Every path to a moved file is rewritten (state machines, Instances, blend trees).
- An old Animation stays where it is, so scripts that find it keep working, and gains an Animator beside it on the same parent.
- `animation:Play()`, `Stop`, `Pause`, `Resume` and `Completed` keep working through that Animator. They report once in Console that `animator:LoadAnimation(animation)` is the new way.
- `examples/hello-animation` and `examples/hello-door` are rewritten in the new shape, and hello-door's script uses `LoadAnimation`.

### Acceptance criteria

- [ ] A project saved before this phase opens, is migrated once, and plays the same in Play.
- [x] hello-door (using `LoadAnimation`) and hello-animation play as before, and tests check their files are saved as the engine writes them.
- [x] One clip that is linked to a take and also keys a sword Part's Transparency plays both.
- [ ] A track from `LoadAnimation` plays over a state machine's Walk, and `Stop(0.3)` fades it out.

## Phase 8: One Animation window

### What to build

**One tab.** One Animation tab replaces the Animation and Animator tabs. It follows the selected Animator (or the Animator of the selected object). A clip opened from the Assets panel opens on its own, with a "Preview on…" model picker.

**Outline.** On the left is an outline of what the Animator has:
- its State Machine, with each state and the clip it plays
- its Animations (those under the Animator or beside it on its parent)

Clicking the State Machine shows the graph, its inspector and the live preview. Clicking a clip or an Animation shows the preview, the drawer, the timeline or dope sheet, and the clip's settings.

**Breadcrumb.** The top of the tab always says where you are (**Hero ▸ State Machine ▸ Walk**). Double-clicking a state opens its clip, and the breadcrumb goes back up. When the tab is narrow, the outline collapses into the breadcrumb.

**Icons and colors.** Animator, State Machine, Animation and clip each have one icon and color, used in the outline, the Explorer, the Assets panel and the pickers. A clip shows a badge, either **Linked to noodle.fbx · Walk** or **Keyed**.

**Animate.** With an object that can animate selected and nothing open, the empty tab shows **Animate <name>**. The same button is in the Explorer's right-click menu and in Properties. One click, as one undo step:
- adds an Animator if the object has none
- writes `assets/animations/<name>/<name>.anim.json`
- adds an Animation named after the clip under the Animator
- opens it with Record on

**+ New Animation** in the outline does the same again for another clip. Importing an animated FBX gives the MeshPart an Animator, a State Machine and an Animation per clip, all in the model's folder. The empty-state checklists shrink to the Animate button and Import….

Everything Phases 4 and 5 did still works: clip ranges, events, the dope sheet, Record and K, the graph, blend trees and Return when.

### Acceptance criteria

- [ ] Starting from a bare door Model: Animate, rotate the Hinge with Record on, tick AutoPlay, and the door swings in Play with no script.
- [ ] The breadcrumb always names the object and what's being edited. Double-clicking a state opens its clip, and the breadcrumb returns to the graph.
- [x] Each kind has the same icon and color in the outline, the Explorer and the Assets panel.
- [ ] The tab is usable at 600×400 and 260×900.

## Phase 9: One Import flow and a clearer File menu

### What to build

**One way in.** One **Import…** handles every asset type. It's reached from the File menu, the ribbon's Insert group and the Assets panel, and by dropping files onto the editor; all four open the same dialog. Textures and sounds import without a dialog, as now, and are reported in Console.

**Tabbed dialog.** A model opens a dialog with tabs, **Objects · Materials · Animation**, showing only the tabs that apply. Each tab scrolls on its own, and Import and Cancel stay pinned at the bottom (Phase 6).

**Summary.** Above the buttons, a summary says in words what Import will create and where, for example: *"Creates: 1 Model with 3 MeshParts · 4 clips in `assets/animations/noodle/` · 1 state machine · an Animator."*

**File menu.** It's grouped into:
- **Project**: New, Open, Save, Version History
- **Import**: Import…
- **Publish**: **Build Game ▸**, the renamed Export Game, with the same targets and the splash option

### Acceptance criteria

- [ ] Every way in opens the same dialog for a model.
- [ ] The summary matches what Import creates (a test compares the two for an animated FBX and for a multi-object glTF).
- [ ] Build Game builds the same game that Export Game did.

## Phase 10: Asset packages

### What to build

**Export Package…** appears for one or more assets selected in the Assets panel, and for an object in the Explorer (exported as a prefab with its assets). It writes one `.axpkg` file: a zip with a manifest and the files at their project paths. Everything they need comes too:
- a state machine brings its clips
- a linked clip brings its FBX
- a model brings its textures and SurfaceAppearance maps
- a prefab brings every asset it uses

**Import… accepts `.axpkg`.** The Import dialog gets a Package tab listing what's inside, and the summary says what will be added.

**Clashes.** The summary lists clashes ("3 files already exist: Walk, Run, Hero.statemachine"), with one choice for all of them:
- **Keep both** (the default): the incoming folder gets a number (`Hero2`), and the paths inside the package are rewritten to match
- **Replace**
- **Skip existing**

Files identical to the project's are skipped silently and aren't counted as clashes. The import is one undo step.

### Acceptance criteria

- [ ] Hero's state machine, exported from one project and imported into an empty one, plays the same: its clips and FBX came with it.
- [ ] Importing the same package twice adds nothing the second time.
- [ ] Keep both, Replace and Skip existing each do what they say when a clip was changed in the destination project.

## Phase 11: Posing skeletal rigs

### What to build

**Posing.** In the Animation window, a skinned model's bones show as a tree in the drawer and as pickable joints in the preview. Rotate a bone, or move the root, with gizmos, and key it (Record, or K). Bone keys are tracks on the one clip, so they play through Animators, state machines, blend trees and AnimationTracks like any clip.

**Locks.** In a linked clip, the bones the take moves show 🔒 and can't be keyed. Bones it doesn't move can be.

**Convert to keys.** This is in the clip's ⋯ menu and is one undo step. It bakes the take's motion over the clip's range into bone keys, at the take's frame rate, dropping keys that don't change the motion. It removes the link, and the badge changes from Linked to Keyed.

### Acceptance criteria

- [ ] A 3-key wave made on an imported rig plays in the preview, in a state machine state, and in Play.
- [ ] A linked clip converted to keys looks the same, then can be edited. Before converting, its locked bones can't be keyed.

## Phase 12: Easing and curve editor

### What to build

**Easing.** Every key has one of these easings:
- Constant or Linear
- Ease In, Out or InOut, each with Sine, Quad, Cubic, Back, Bounce or Elastic
- Bezier, with tangent handles

**Curves view.** It toggles with the dope sheet and shows the selected tracks' value curves. It has zoom and pan, draggable keys and tangent handles, and frame-all. Every track kind uses it: bone, property and UI tracks. The `anim` crate evaluates the same curves.

### Acceptance criteria

- [ ] Changing a key's easing changes the motion in the preview and in Play.
- [ ] Bezier tangents dragged in the curve editor are saved and reloaded.

## Phase 13: UI Designer artboard

### What to build

The **UI Designer** dock tab shows the selected ScreenGui on an artboard at its design size (DesignWidth×DesignHeight, with presets for 1920×1080, 1280×720, phone portrait and landscape, and custom sizes), with zoom and pan. It has a palette of UI elements to drag on, a hierarchy, selection handles with snapping (shared with `ui_handles.rs`), and the Style (CSS) inspector. Edits go through `EditorApp::change` and the stylesheet tabs, so the Scene View and Play match.

### Acceptance criteria

- [ ] A menu with a Frame, a TextLabel and two Buttons can be built and styled in the designer only, and it looks the same in Play.
- [ ] Every designer edit is undoable.

## Phase 14: UI Event Graph

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
