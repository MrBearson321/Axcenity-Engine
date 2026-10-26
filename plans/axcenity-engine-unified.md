# Plan: Axcenity Engine, unified remainder of v2 and v3

> Sources: `plans/axcenity-engine-v2.md` (PRD `prd/axcenity-engine-v2.md`) and `plans/axcenity-engine-v3.md` (the developer's requests of 2026-10-22 and 2026-10-23), and the developer's requests of 2026-10-25 (Phases 8a to 8d). This plan holds only what has not been built, as of 2026-10-25 (commit `a9b6638`). The phases run in order. Each phase names the v2 or v3 phase it came from. Where the text changed from the source plan, the change is marked **Changed**.
>
> Built and committed, but not yet ticked: v2 Phase 12 and v3 Phases 4, 6, 7 and 8. Their open criteria are under **Open checks** below. They need a hands-on check, not new work.

## Architectural decisions

v1, v2 and v3 rules still hold. These are the ones the remaining phases depend on.

- **Crates**: `character` already exists. `nav` arrives in Phase 12. It may query `physics` and `terrain`, and `physics` and `character` must not depend on it. `save` arrives in Phase 17. It depends on `core` only and never reads or writes the editor project.
- **Dependency rules**: `player` never depends on `editor`, `egui` or `blocks`. `render` receives only a draw list. The graph-to-Lua compiler lives next to the blocks translator, so generated Lua is ordinary Lua for the player.
- **Character movement**: controllers move a character kinematically and own its camera during Play. The host steps them after input and before scripts, then holds them kinematic just before the physics step. Walk, run and crouch are built-in modes. Jump, slopes and step-up are movement on those modes. A custom mode is a name: while it's active, scripts and blocks supply the motion.
- **Frame order**: input, then character controllers and navigation agents, then scripts, physics, touch events, UI layout, render. Visual-debug overlays draw in the editor viewport only.
- **Graphs**: there is one generic node-graph editor (Phase 11). The UI Event Graph and behavior trees (Phase 16) both use it. **Changed**: v2 had behavior trees with their own graph editor. Animator state machines stay the animation system and are not node graphs.
- **AI**: behavior trees are their own asset. Scripts and blocks can set an agent's destination without a tree.
- **Saves**: a slot holds player data, world state, the active checkpoint and settings. Save files live in the player's data for that game, never in the project folder. Each file has a version and a migration. Editor Play and Stop still restore the editing world.
- **Project files**: new optional files (`*.uigraph.json`, `*.axpkg`, the string table, behavior-tree assets) don't bump `project::FORMAT_VERSION`. A changed shape of an existing file does, with a migration that keeps a copy of the previous files.
- **Scripting**: one Lua runtime. Any creator-facing Lua added in a phase has matching blocks. Output keeps `script:line` errors.
- **Dependencies**: **Changed**: `.axpkg` needs no new dependency. `zip` is already pinned in the workspace and used to read and write archives (`editor/src/ambient_cg.rs`). Controller (gamepad) input in Phase 19 needs one new pinned dependency, with its licence checked as Apache-2.0-compatible.
- **Platforms**: Windows, macOS and Linux.

---

# Open checks (built, not yet ticked)

Tick these in the source plan once they've been checked by hand. Where a test already covers a criterion, it is named, so a failure can be traced.

**v2 Phase 12, third-person walk and run** (`examples/hello-character`):

- [x] During Play, walk and run move the character across a flat floor at different speeds, and it doesn't tip over or fall. Tests: `character` `walk_and_run_move_at_different_speeds_and_stay_upright`, player `hello_character_walks_runs_and_switches_from_lua_and_blocks`.
- [x] The third-person camera follows behind the character during Play, and after Stop the fly camera is back. Test: `the_camera_starts_behind_the_character`. Following and the handoff on Stop are checked only by hand.
- [x] An unanchored crate still falls and collides, and the controller's speeds don't change crate gravity or friction. Tests: `a_wall_stops_the_character_and_a_crate_still_falls`, `walk_speed_does_not_change_how_a_crate_falls`.
- [x] The controller saves, reloads, and switches between walk and run from Lua and from blocks. Tests: the save round trip in `project/src/world_file.rs`, and the hello-character player test.

**v3 Phases 4 and 8, Animation window:**

- [ ] Everything the old panel did still works in the one Animation tab: clip ranges, events, state machines, blend trees and Return when. Test: `a_return_setting_keeps_its_transition_in_step`. The rest is checked only by hand.
- [ ] Starting from a bare door Model: Animate, rotate the Hinge with Record on, tick AutoPlay, and the door swings in Play with no script. Tests: `animate_makes_an_animator_a_clip_and_an_animation_as_one_undo_step` and `a_hinge_turn_recorded_in_the_preview_plays_the_same_in_the_game` cover parts of this. AutoPlay in Play is checked only by hand.
- [ ] The breadcrumb always names the object and what's being edited. Double-clicking a state opens its clip, and the breadcrumb goes back to the graph. Test: `the_animation_tab_says_where_you_are_and_fits_any_shape`.
- [ ] The tab is usable at 600×400 and 260×900. Same test.

**v3 Phase 6, Import dialog:**

- [ ] An animated FBX with texture maps imports in an 800×600 editor window, and Import is visible and clickable without scrolling. Test: `the_whole_dialog_fits_an_800_by_600_window_with_import_clickable`.

**v3 Phase 7, one clip format:**

- [ ] A project saved before format 3 opens, is migrated once, and plays the same in Play. Test: `an_older_project_moves_its_files_rewrites_their_paths_and_gains_animators_once` checks the files. Playing the same is checked only by hand.
- [ ] A track from `LoadAnimation` plays over a state machine's Walk, and `Stop(0.3)` fades it out. Test: `a_track_plays_over_the_state_machine_and_stop_fades_it_out`.

---

# Milestone 1: Character controllers

## Phase 1: Jump, slopes, and steps

**From**: v2 Phase 13 (stories 16, 20)

### What to build

The third-person controller jumps, walks up and down slopes within a limit, and steps onto low obstacles. It falls under gravity when it walks off an edge. **Changed**: today it snaps to flat ground only. A slope steeper than the limit stops the character instead of launching it, and a step taller than the limit blocks it. Jump height and the slope and step limits are properties. Jump is a held key in `MoveInput` (Space during Play), and Lua and blocks can make it jump. Physics parts are unchanged.

### Acceptance criteria

- [x] Jump rises and lands on the floor, and a second jump in the air does nothing.
- [x] Walking off a ledge falls to the floor below.
- [x] The character walks up a slope and a step that are under their limits, and is stopped by a steeper slope and a taller step.
- [x] Changing those limits changes what the character can cross, and a falling crate is unaffected.

---

## Phase 2: Crouch

**From**: v2 Phase 14 (story 16)

### What to build

Crouch is a built-in mode on the third-person controller. While crouched, the character's collision height and the camera drop, and they come back up when it stands. The character can't stand while something overhead would cut into the standing height, and it stays crouched until the way is clear. Scripts and blocks set crouch. Walk and run still work while crouched, at a slower speed.

### Acceptance criteria

- [x] Crouching lowers the character and the camera, and standing restores both.
- [x] Under a low ceiling, standing is refused until the character is clear.
- [x] Crouched movement is slower than walking, and Lua and blocks can enter and leave crouch.

---

## Phase 3: First-person controller

**From**: v2 Phase 15 (stories 17, 19)

### What to build

A `FirstPersonController` is its own Instance class, not a mode of the third-person controller. It walks, runs, jumps, handles slopes and steps, and crouches, with its own properties. During Play its camera sits at the character's eyes and looks where the player looks. A character has one controller or the other. The third-person controller is unchanged. Shared movement code lives in `character`, not in two copies.

### Acceptance criteria

- [x] A first-person controller walks, runs, jumps, crouches, and crosses the same slopes and steps as the third-person controller.
- [x] Its camera stays at the eyes during Play, and Stop restores the fly camera.
- [x] Inserting a first-person controller doesn't turn an existing third-person controller into one, and each saves under its own class.

---

## Phase 4: Custom movement modes

**From**: v2 Phase 16 (story 18)

### What to build

Either controller accepts a named custom movement mode in `Movement`. While that mode is active, the built-in walk, run and crouch motion doesn't run, and scripts and blocks supply the motion each frame. **Changed**: today any `Movement` value other than Walk or Run moves at `WalkSpeed`. A custom name will stop built-in motion instead. Clearing the mode returns to the built-in mode that was active before. The mode name can be read back. Jump, slopes and steps keep working in the built-in modes.

### Acceptance criteria

- [x] Setting a custom mode stops built-in walking, and motion supplied by a script moves the character.
- [x] Blocks can set and clear the same mode, and the current mode name can be read back.
- [x] Clearing the mode restores walk, run or crouch, including jumping and stepping.

---

# Milestone 2: Import and packages

## Phase 5: One Import flow and a clearer File menu

**From**: v3 Phase 9

### What to build

**One way in.** One **Import…** handles every asset type. It's reached from the File menu, the ribbon's Insert group and the Assets panel, and by dropping files onto the editor. All four open the same dialog. Textures and sounds import without a dialog, as now, and are reported in Console.

**Tabbed dialog.** A model opens a dialog with tabs, **Objects · Materials · Animation**, showing only the tabs that apply. Each tab scrolls on its own, and Import and Cancel stay pinned at the bottom.

**Summary.** Above the buttons, a summary says in words what Import will create and where. For example: *"Creates: 1 Model with 3 MeshParts · 4 clips in `assets/animations/noodle/` · 1 state machine · an Animator."*

**File menu.** It's grouped into:
- **Project**: New, Open, Save, Version History
- **Import**: Import…
- **Publish**: **Build Game ▸**, the renamed Export Game, with the same targets and the splash option

### Acceptance criteria

- [x] Every way in opens the same dialog for a model.
- [x] The summary matches what Import creates. A test compares the two for an animated FBX and for a multi-object glTF.
- [x] Build Game builds the same game that Export Game did, and the Learn page and docs say Build Game.

---

## Phase 6: Asset packages

**From**: v3 Phase 10

### What to build

**Export Package…** appears for one or more assets selected in the Assets panel, and for an object in the Explorer, which is exported as a prefab with its assets. It writes one `.axpkg` file: a zip with a manifest and the files at their project paths. Everything the assets need comes too:
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

- [x] Hero's state machine, exported from one project and imported into an empty one, plays the same, because its clips and FBX came with it.
- [x] Importing the same package twice adds nothing the second time.
- [x] Keep both, Replace and Skip existing each do what they say when a clip was changed in the destination project.
- [x] Asset validation (v2 Phase 7) reports no broken references after any of the three choices.

---

# Milestone 3: Animation authoring

## Phase 7: Posing skeletal rigs

**From**: v3 Phase 11

### What to build

**Posing.** In the Animation window, a skinned model's bones show as a tree in the drawer and as joints you can pick in the preview. Rotate a bone, or move the root, with gizmos, and key it (Record, or K). Bone keys are tracks on the one clip, using the existing `Bone` track kind. They play through Animators, state machines, blend trees and AnimationTracks like any clip.

**Locks.** In a linked clip, the bones the take moves show 🔒 and can't be keyed. Bones it doesn't move can be.

**Convert to keys.** This is in the clip's ⋯ menu and is one undo step. It bakes the take's motion over the clip's range into bone keys, at the take's frame rate, and drops keys that don't change the motion. It removes the link, and the badge changes from Linked to Keyed.

### Acceptance criteria

- [x] A 3-key wave made on an imported rig plays in the preview, in a state machine state, and in Play.
- [x] Before converting, a linked clip's locked bones can't be keyed. After Convert to keys, the clip looks the same and can be edited.

---

## Phase 8: Easing and curve editor

**From**: v3 Phase 12

### What to build

**Easing.** Every key has one of these easings. Today a key is only Step or Linear, and those stay as Constant and Linear:
- Constant or Linear
- Ease In, Out or InOut, each with Sine, Quad, Cubic, Back, Bounce or Elastic
- Bezier, with tangent handles

New easings and tangents are optional fields, so existing clips read unchanged and the project format doesn't bump.

**Curves view.** It toggles with the dope sheet and shows the selected tracks' value curves. It has zoom and pan, keys and tangent handles you can drag, and frame-all. Every track kind uses it: bone, property and UI tracks. The `anim` crate evaluates the same curves.

### Acceptance criteria

- [x] Changing a key's easing changes the motion in the preview and in Play.
- [x] Bezier tangents dragged in the curve editor are saved and reloaded.
- [x] A clip saved before this phase opens and plays the same.

---

## Phase 8a: Imported clips you can see, tidier trees, no leftover imports

**From**: the developer's requests of 2026-10-25

### What to build

**An imported clip's keys.** A linked clip (one imported from a take) shows **Dope sheet · Curves** under its timeline, as a keyed clip does. They show the take's motion over the clip's range, one row per bone the take moves. They are read-only and are worked out with `anim::convert_to_keys` on a copy, so they are exactly the keys Convert to keys would make. A note over them says "Imported motion: Convert to keys to edit it", with the button beside it. Selecting a key or a curve works, but nothing can be dragged or deleted.

**Folding.** In the drawer's bone tree, a bone with bones under it has a ▸/▾ to fold them. Collapse all and Expand all are in the drawer's header. A tree opens with its first three levels showing. A folded bone shows a dot when a bone under it has keys. Folds are kept per model while the editor runs. The outline's **State Machine** and **Animations** fold the same way.

**Leftover imports.** Undoing an import removes the files it copied and wrote: the model, its textures, its clips and its state machine. Redo puts them back. This uses `History::record_blobs`, as packages do. **Changed**: before this phase, undo left the copied files in the project. Open ▾ marks a clip or state machine that nothing in the scene uses as **not used**, and its ⋯ menu still deletes it.

### Acceptance criteria

- [x] An imported FBX clip shows its bones' keys in the dope sheet and their curves in Curves, and they can't be changed until Convert to keys.
- [x] A Mixamo rig's bone tree folds and unfolds, and Collapse all leaves only the root showing.
- [x] Importing an animated FBX and then undoing leaves no files from it in the project, and redo brings the MeshPart and its files back.
- [x] The keys and curves shown match `anim::convert_to_keys` for the same take and clip range, including when the clip starts or ends partway through the take.
- [x] Imported keys and curves can be selected, but never moved, deleted or otherwise changed. Convert to keys makes editable keys without changing the clip's motion.
- [x] Undo removes only the files that import made, and keeps files and assets that were already there. Redo brings back the same assets and references, with no duplicates.
- [x] Folding and unfolding works on nested bones, and each tree stays folded as it was when you switch between models and clips during a session.
- [x] Open ▾ marks a clip or state machine as **not used** when nothing in the scene uses it, and the mark updates when something starts or stops using it.

---

## Phase 8b: Easier posing and smooth keys

**From**: the developer's requests of 2026-10-25

### What to build

**Steady rotation.** Dragging a rotate ring turns the bone by how far the mouse moves along the ring on screen: a steady number of degrees per point. A ring seen nearly edge-on no longer spins. Today the turn comes from where the mouse ray meets the ring's plane. Hold Shift while dragging for fine turns (a quarter of the speed). The Scene View's rotate tool gets the same drag.

**Snapping controls.** Above the preview, beside Move and Rotate, there are two snap settings. Turn snap is Off, 1°, 5°, 15° or 45°, and starts at 5°. Move snap is Off, 0.01, 0.05, 0.1 or 0.5 studs, and starts at 0.05. Holding Ctrl during a drag flips snapping for that drag. The settings are kept while the editor runs.

**World and Local axes.** A toggle sets whether the gizmo turns around the world's axes or the bone's own.

**Smooth keys.** New keys use the **New keys** easing picked in the dope sheet's toolbar. That applies to keys from Record, K, posing and Add track. The choices are Smooth (Ease InOut, Sine), Linear and Constant, and Smooth is the default. **Changed**: new keys were always Linear, so motion between two poses started and stopped with a jerk. Right-clicking selected keys has **Easing ▸**, which sets the easing of all of them at once. Existing clips don't change.

### Acceptance criteria

- [x] A drag of the same length on screen turns a bone by the same angle, whether its ring faces the camera or is nearly edge-on.
- [x] With turn snap at 15°, a bone only turns in 15° steps. With it Off, or with Ctrl held, it turns freely.
- [x] Two keys made by posing play with ease-in and ease-out in the preview and in Play. Changing their easing to Linear makes the motion linear.
- [x] Move snapping: With move snap set to 0.05 studs, dragging moves the bone in 0.05-stud increments. Ctrl temporarily disables snapping, and releasing it restores the configured behavior.
- [x] Snap override: Ctrl reverses the current snapping state for both Move and Rotate, regardless of whether snapping is configured Off or to a specific increment.
- [x] World/Local axes: Switching between World and Local changes the gizmo's orientation and rotation behavior without unexpectedly moving or rotating the selected bone.
- [x] Shift precision: Holding Shift reduces rotation sensitivity to one-quarter of normal; releasing it restores normal sensitivity without a jump in rotation.
- [x] New-key easing: Record, K, posing, and Add track all use the selected New keys easing setting. Changing the setting affects newly created keys only.
- [x] Existing-key easing: Applying Easing to selected keys updates all selected keys, while unselected keys and existing clips remain unchanged.
- [x] Consistent playback: Smooth, Linear, and Constant easing behave identically in the editor preview and Play mode.
- [x] Undo/redo: Changing key easing and creating keys with the selected easing can be undone and redone without losing the original values or easing.

---

## Phase 8c: Moving joints and inverse kinematics

**From**: the developer's requests of 2026-10-25

### What to build

**Move with IK.** The Move tool works on every bone, not only roots. Moving a joint that isn't a root turns the bones above it so that the joint reaches where it's dragged. No bone gets longer or shorter. The chain is the joint's parent and grandparent by default (forearm and upper arm for a hand). A **Chain** setting (1 to 4 bones) sets how many bones turn. The joint is dragged on a plane facing the camera, or along a gizmo axis. A limb keeps bending the way it bent: a knee stays forward and an elbow stays back.

**Pins.** Right-click a joint and choose **Pin** to keep it where it is in the world. When the root or another chain moves, a pinned joint's own chain turns to keep it in place. So moving the hips down with both feet pinned bends the knees. Pins are shown as a pin icon, are kept per clip while the editor runs, and aren't saved in the clip.

**Keys.** With Record on, a move keys every bone it turned, at the playhead. **K** does the same. The keys are ordinary `Bone` rotation keys, so they play everywhere bone keys play and need nothing at runtime.

**Solver.** The solver is in the `anim` crate (`anim/src/ik.rs`) and is plain math with its own tests. It solves two-bone chains exactly, with the current bend as the pole. Longer chains use FABRIK and then turn each bone back toward its old twist. If the target can't be reached, the chain points straight at it.

IK that runs during Play (feet on slopes, look-at) is not in this phase.

### Acceptance criteria

- [x] Dragging a hand moves it to the mouse with the arm's two bones turning, and every bone keeps its length.
- [x] With both feet pinned, moving the hips down bends the knees forward and the feet stay where they were.
- [x] A pose made with IK and keyed with Record plays the same in the preview and in Play.
- [x] Unreachable targets: When the target is beyond the chain's reach, the chain extends toward it without stretching bones or producing invalid transforms.
- [x] Bend stability: Moving a joint repeatedly doesn't cause elbows or knees to flip unexpectedly. The solver preserves the existing bend direction unless the pose makes that impossible.
- [x] Chain length: Chain settings from 1–4 affect the correct ancestors, and all affected bones retain their original lengths.
- [x] Pin lifecycle: Pins persist when switching between clips during the editor session, but are isolated per clip and disappear when the editor session ends.
- [x] Pin conflicts: When multiple pins or overlapping IK chains constrain the same bones, the solver has a deterministic result and never silently moves a pinned joint.
- [x] Keying scope: Record and K key only bones whose rotations changed during the IK operation. Unaffected bones receive no redundant keys.
- [x] Solver robustness: Zero-length bones, targets at the joint position, and nearly straight chains don't produce NaNs, crashes, or sudden extreme rotations.
- [x] Undo/redo: Undoing an IK pose restores the previous transforms and keys; redoing it restores the same pose.

---

## Phase 8d: Blueprint-style state machines

**From**: the developer's requests of 2026-10-25. **Changed**: the decision that "Animator state machines are not node graphs" still holds for the file and the runtime. This phase changes only how the graph is drawn and edited. It stays states and transitions, in the same file format.

### What to build

**Nodes and wires.** The state machine graph looks and works like the script Graph view (`block_editor/graph.rs`):
- Each state is a node with its name, what it plays (a clip, or "blend tree · Speed") and a progress bar while previewing.
- Each node has an input pin on the left and an output pin on the right.
- An **Entry** node is wired to the default state. Rewiring Entry changes the default state.
- **Any State** is a node with only an output pin.
- Drag from an output pin to a node to make a transition. Dropping on empty space opens a menu: a new state playing one of the project's clips (searchable), or an empty state.
- Drop a clip from Open ▾ or the Assets panel onto the graph to make a state that plays it.
- Right-click → Make Transition still works.

**Conditions on the wire.** Each wire has a label that says when it happens, in words: "Speed > 0.1", "Jump", "when Walk finishes" or "always". Clicking the label opens an editor right there:
- **When…** rows: a parameter (or **New parameter…**, typed inline), a test that fits the parameter's type, and a value.
- **+ condition**.
- **Wait for the clip to finish** (exit time).
- **Blend** (Instant, Smooth or Slow).
- **Delete transition**.

The inspector keeps the full settings.

**Parameters panel.** The graph's left side lists the parameters as chips with type icons, with **+** to add one. Drag a chip onto a wire to add a condition on it.

**Explained.** A state machine with no transitions shows a three-step guide in the graph: add states, wire them, and say when on the wire. The inspector's headers say what each part is. For example: "Transition: when all of these are true, blend from Walk to Run."

**File.** The format is unchanged. Entry's position is saved in a new optional `entry_position` field, so older files read the same.

### Acceptance criteria

- [x] A transition made by dragging from Idle's output pin to Walk, with "Speed > 0.1" set on its wire label, plays in the live preview when the Speed slider passes 0.1, and in Play.
- [x] Rewiring Entry to another state makes that state the default.
- [x] A state machine saved before this phase opens with the same states, transitions and positions.

---

# Milestone 4: UI authoring

## Phase 9: UI Designer artboard

**From**: v3 Phase 13

### What to build

The **UI Designer** dock tab shows the selected ScreenGui on an artboard at its design size (`DesignWidth`×`DesignHeight`), with presets for 1920×1080, 1280×720, phone portrait and landscape, and custom sizes, plus zoom and pan. It has a palette of UI elements to drag on, a hierarchy, selection handles with snapping (shared with `ui_handles.rs`), and the Style (CSS) inspector. Edits go through `EditorApp::change` and the stylesheet tabs, so Scene View and Play match. It follows the responsive-panel rules: it stays usable at 260×900 and 900×260.

### Acceptance criteria

- [x] A menu with a Frame, a TextLabel and two Buttons can be built and styled in the designer alone, and it looks the same in Play.
- [x] Every designer edit is undoable.

---

## Phase 10: TweenService

**From**: new. **Changed**: v3 Phase 14's Tween node compiles to Lua, and the engine has no Lua tween API yet.

### What to build

A `TweenService` service with `TweenService:Create(instance, tweenInfo, goals)` returns a `Tween` with `Play`, `Cancel`, `PlaybackState` and `Completed`. `TweenInfo.new(time, easingStyle, easingDirection, repeatCount, reverses, delayTime)` uses the easing styles from Phase 8 through the same `anim` easing functions. It tweens numbers, Vector3, Color3, UDim2 and CFrame-like properties on Parts and UI elements. Matching blocks translate to it. It runs in the player with no editor dependency.

### Acceptance criteria

- [ ] A script tweens a Part's Position and a Frame's BackgroundTransparency over one second with Quad Out, and `Completed` fires once.
- [ ] `Cancel` stops a tween where it is, and a second tween on the same property replaces the first.
- [ ] The tween blocks produce the same Lua and the same motion, and the API reference lists TweenService.

---

## Phase 11: UI Event Graph

**From**: v3 Phase 14

### What to build

A generic node-graph editor, built to be reused by behavior trees in Phase 16: nodes, typed pins, wires, comments, pan and zoom, box select, copy and paste, and a searchable node menu. Then a UI node library:
- **events**: Button Activated, Hovered, TextBox FocusLost, Screen Shown, Every Frame
- **actions**: Set Property, Show Page (sets a `PageLayout`'s `CurrentPage`), Set Visible, Tween (Phase 10), Play Sound, Fire RemoteEvent, Print
- **flow**: Branch, Sequence, Delay
- **data**: Get Property, Get Text, math, compare, string join, variables

Each ScreenGui has one graph saved as `<ScreenGui>.uigraph.json`. It compiles to a generated LocalScript under the ScreenGui, which is read-only and regenerated on save. An Event Graph sub-tab sits in the UI Designer. The designer's Live Preview runs the compiled Lua in a sandboxed preview world, so clicking a button in the designer runs its graph.

### Acceptance criteria

- [ ] "Button Activated → Show Page Settings" works in Live Preview and in Play.
- [ ] A bad graph (for example, an unconnected required pin) shows errors on its nodes, not Lua errors.
- [ ] The generated LocalScript is ordinary Lua that the player runs without the editor.

---

# Milestone 5: Navigation and AI

## Phase 12: Navigation mesh

**From**: v2 Phase 17 (story 21)

### What to build

A new `nav` crate. The world builds one navigation mesh from collidable parts, terrain and colliding foliage. A part that doesn't collide, and foliage marked visual, leave no walkable surface. Rebuilding after a move or a terrain edit updates the mesh. A visual-debug toggle (beside v2 Phase 6's overlays) shows the mesh in the editor. Nothing walks on it yet.

### Acceptance criteria

- [ ] A floor of parts and a terrain surface both make a walkable mesh, and a gap or a steep drop isn't walkable.
- [ ] A non-colliding part and visual foliage add no walkable surface.
- [ ] Moving a floor part or sculpting terrain, then rebuilding, changes the mesh to match.

---

## Phase 13: Agents and pathfinding

**From**: v2 Phase 18 (stories 22, 23, 26, 27)

### What to build

A `NavigationAgent` Instance finds a path on the mesh to a destination and walks it. Scripts and blocks set the destination, or set a velocity to move the agent without a path. The editor inserts an agent and can draw its current path with a visual-debug path toggle. An unreachable destination doesn't move the agent, and the failure is reported. Avoiding obstacles beyond following the path is the next phase.

### Acceptance criteria

- [ ] An agent placed in the editor walks along the mesh to a destination set in Properties, from Lua, and from blocks.
- [ ] A destination off the mesh doesn't move the agent, and the failure is visible.
- [ ] Setting a velocity moves the agent without a path, and the path toggle draws the path of an agent that has one.
- [ ] The path overlay doesn't change the route the agent takes.

---

## Phase 14: Obstacle avoidance

**From**: v2 Phase 19 (story 22)

### What to build

While an agent follows a path, it steers around obstacles that weren't in the mesh when the path was built, such as a part that moved into the way. It returns to the path when the way is clear. If a blocked path can't be steered around, the agent stops and reports it. Path drawing still shows the path being followed.

### Acceptance criteria

- [ ] An agent following a path steers around a part pushed into that path and rejoins the path afterward.
- [ ] An agent facing a blockage it can't steer around stops and reports that it's blocked.
- [ ] With nothing in the way, the agent follows the same path as in Phase 13.

---

## Phase 15: Perception

**From**: v2 Phase 20 (story 24)

### What to build

A navigation agent can see and hear. Sight has a range and an angle, and colliders block it. Hearing has a range, and walls don't block it. A creator sets those distances. When the player character (the one whose controller owns the camera) comes into sight or hearing, the agent reports which sense fired. **Changed**: "makes a sound" is defined as a `Sound` playing inside the player character's model. Ranges of zero sense nothing.

### Acceptance criteria

- [ ] An agent reports the player character walking into its view, and doesn't report one hidden behind a collider or outside the angle.
- [ ] An agent reports the player character making a sound inside hearing range, including one behind a wall.
- [ ] Sight range, sight angle and hearing range save with the agent and change what it notices.

---

## Phase 16: Behavior trees

**From**: v2 Phase 21 (story 25)

### What to build

A behavior tree is its own asset, edited in the generic graph editor from Phase 11, and a navigation agent references it. The tree can test perception, set a destination and wait. An agent with a tree runs it during Play. An agent without a tree still moves when scripts set a destination. Animator state machines are unchanged and can't be used as the AI graph.

### Acceptance criteria

- [ ] A creator can add, connect and save nodes that test sight and set a destination, and an agent using that tree chases the player when it sees them.
- [ ] An agent with no tree still pathfinds when a script sets its destination.
- [ ] Opening a state machine still edits animation states, and assigning one to an agent doesn't run it as AI.

---

# Milestone 6: Save and load

## Phase 17: Save slots and checkpoints

**From**: v2 Phase 22 (stories 28, 29, 31, 32)

### What to build

A new `save` crate. A `Checkpoint` Instance marks a place in the world. Scripts and blocks write a numbered save slot that stores player data, world state, the active checkpoint and settings, then later load that slot and restore them. Restoring a checkpoint puts the character back at that checkpoint with the state captured there. The file lives in the player's data for that game, not in the project folder. Saving during editor Play doesn't change the project, and Stop still restores the editing world. Loading a missing slot fails clearly.

### Acceptance criteria

- [ ] Saving a slot and loading it restores player data, moved world state, the active checkpoint and settings.
- [ ] Restoring a checkpoint places the character there with the state captured for it.
- [ ] The save file is outside the project folder, the project files are unchanged after a save, and Stop still restores the pre-Play world.
- [ ] Lua and blocks can save, load and restore a checkpoint, and loading a missing slot fails without touching the current world.

---

## Phase 18: Save migration

**From**: v2 Phase 23 (story 30)

### What to build

Save files carry a version. Loading a slot from the previous version migrates it and then restores it. A save newer than the game understands is refused and doesn't overwrite the slot. This phase includes one real migration from the previous save version. Project migration is a separate path and stays as it is.

### Acceptance criteria

- [ ] A slot written by the previous save version loads, migrates, and restores the same player, world, checkpoint and settings.
- [ ] A slot newer than this game understands is refused, and the slot file is unchanged.
- [ ] A current slot still round-trips without a migration step, and project folders are never used as save files.

---

# Milestone 7: UI runtime

## Phase 19: Keyboard and controller navigation

**From**: v2 Phase 24 (stories 34, 33)

### What to build

Existing menus and HUDs can be navigated with the keyboard and a game controller. **Changed**: the engine has no controller input yet. This phase adds it to the player and editor Play through one new pinned, Apache-2.0-compatible dependency. Focus moves in a documented order, the focused control is visible, and the confirm button activates it the way a click does. A mouse click still works. Text boxes take keyboard input while focused and hand focus back when finished. HUDs, clicks, text and layout scaling are unchanged for a layout that never uses focus.

### Acceptance criteria

- [ ] The keyboard and a controller both move focus through a menu in the same order and activate the focused button.
- [ ] A focused text box receives typing, and leaving it fires the same `FocusLost` that clicking away does.
- [ ] A HUD that isn't navigated still shows, clicks and scales as it does today.

---

## Phase 20: Localization

**From**: v2 Phase 25 (story 35)

### What to build

A project has a string table the creator can add entries to, with a value per language. UI text can refer to an entry, set in Properties and in the UI Designer. The game shows the value for the active language. A missing entry shows the source text authored on the control. Changing the active language updates the visible text. The table saves with the project as its own file and doesn't bump the project format version.

### Acceptance criteria

- [ ] A label bound to a table entry shows that entry's text in the active language, in the UI Designer and in Play.
- [ ] Switching the active language updates every bound control.
- [ ] A missing entry shows the control's source text, and the table survives save and reload.

---

## Phase 21: Game UI accessibility

**From**: v2 Phase 26 (story 36)

### What to build

Game UI settings, separate from the editor's appearance settings, control text scale, a higher-contrast option and reduced motion. They apply to HUDs and menus in Play and in the UI Designer, and they are part of the settings a save slot stores (Phase 17). Each control has an accessible name, which defaults to its `Name`, that navigation and scripts can read. Reduced motion turns off UI motion that's only cosmetic, including UI tweens and UI animation tracks. Contrast and scale don't move world geometry.

### Acceptance criteria

- [ ] Raising text scale, turning on higher contrast and turning on reduced motion each change the game UI and leave the 3D world as it was.
- [ ] Those settings persist in a save slot and restore on load, and they don't change the editor's theme.
- [ ] A script can read each control's accessible name, and keyboard focus still moves in the Phase 19 order.
