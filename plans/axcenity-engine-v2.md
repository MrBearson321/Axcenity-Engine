# Plan: Axcenity Engine v2

> Source PRD: `prd/axcenity-engine-v2.md`. Stories 2, 3, 5, 11, 12, 13, and 33 already ship in v1. They are regression checks on the phases below, not phases of their own. This plan does not rebuild the renderer, the scripting sandbox, or export.

## Architectural decisions

These apply to every phase. v1's crate rules still hold.

- **Workspace**: the v1 crates stay. A new crate is created in the first phase that needs it, not before.
  - `character` (phase 12): kinematic character controllers. It may query physics. `physics` must not depend on it.
  - `nav` (phase 17): navigation meshes, pathfinding, avoidance, agents, perception, and behavior trees. It may query physics and terrain. `physics` and `character` must not depend on it.
  - `save` (phase 22): runtime save slots. It depends on `core` only. It must not read or write the editor project.
- **Dependency rules**: `player` never depends on `editor`, `egui`, or `blocks`. `render` receives only a draw list. No cycles; shared Instance types live in `core`.
- **Data model**: new gameplay objects are Instances in the arena. First-person and third-person are separate classes, `FirstPersonController` and `ThirdPersonController`. A `NavigationAgent` pathfinds. A `Checkpoint` marks a restore point. Foliage is painted terrain data (which plant or prop, density, and whether it collides), not one Instance per plant.
- **Character movement**: controllers move a character kinematically and own its camera during Play. They are not dynamic physics bodies. The host steps them after input and before the physics step. Walk, run, and crouch are built-in modes. Jump, slopes, and step-up are movement on those modes. A custom mode is a name; while it is active, scripts and blocks supply the motion. The editor fly camera is unchanged outside Play.
- **AI**: behavior trees are their own asset and editor graph. Animator state machines stay the animation system. Scripts and blocks can set an agent's destination without a tree.
- **Saves**: a slot holds player data, world state, the active checkpoint, and settings. Files live in the player's data for that game, never in the project folder. Each file has a version and a migration. Editor Play and Stop still restore the editing world.
- **Editor-only tools**: breakpoints, the profiler, visual-debug overlays, and asset validation run in the editor. Exported games do not ship them. The player build never arms a breakpoint.
- **Projects**: creating, opening, and project settings stay as they are. A format version newer than the engine understands still refuses to open. When a phase changes the on-disk project shape, it bumps the format version, migrates the previous version, and keeps a copy of the previous files. A new optional file does not bump the version.
- **Scripting**: one Lua runtime. Any creator-facing Lua added in a phase has matching blocks that translate to it. Output keeps `script:line` errors.
- **UI**: existing ScreenGui, layout, clicks, and text stay. Focus navigation, the string table, and accessibility settings are game UI, separate from the editor's appearance settings.
- **Frame order**: input, then character controllers and navigation agents, then scripts, physics, touch events, UI layout, render. Visual-debug overlays draw in the editor viewport only and do not change this order.
- **Platforms and license**: Windows, macOS, and Linux. New dependencies are pinned in the workspace and stay Apache-2.0-compatible.

---

# Milestone 1: Editor reliability

## Phase 1: Asset command history

**User stories**: 1, 2, 11, 12

### What to build

Scene edits, terrain strokes, and property-clip edits already undo as one step, and saving already keeps that history. Extend the same history to the asset edits that are still permanent: creating, renaming, duplicating, and deleting animation clips, animator controllers, and prefabs. One action is one step. Undo and redo restore the asset and the panel that lists it. Saving still does not clear history. Selection, snapping, gizmos, grouping, duplication, multi-selection, and the animation timeline keep working.

### Acceptance criteria

- [x] Creating, renaming, duplicating, and deleting a clip, a controller, and a prefab are each one undo step, and redo puts the change back.
- [x] Undoing a delete restores the asset's contents, and the asset panel shows it again without reopening the project.
- [x] Saving the project leaves undo and redo available for those asset steps.
- [x] Existing scene, terrain, and property-clip undo still works, and the world editor and animation timeline still edit as they do today.

---

## Phase 2: Project migration

**User stories**: 4, 5

### What to build

Opening a project whose format version is older than the engine's upgrades it in place and keeps a copy of the previous files. The upgraded project opens as the current version. A project newer than this engine still refuses to open and tells the creator to update Axcenity. Creating a project, opening a current one, and changing project settings stay as they are. This phase includes one real migration from the previous version so the path is exercised, even where the world looks the same afterward. Later phases that change the on-disk shape add their own migration step on top of this path.

### Acceptance criteria

- [x] Opening an older project rewrites it to the current format version, and the world, scripts, and assets still load.
- [x] A copy of the previous files is kept beside the upgrade and is not overwritten by a later save.
- [x] Opening a project newer than this engine fails with a message to update Axcenity, and the folder is left unchanged.
- [x] New Project, Open, recent projects, and Project Settings behave as they do today on a current project.

---

## Phase 3: Script stack traces

**User stories**: 6

### What to build

A script error during Play still shows the script and line in Output, and clicking it still opens that line. Output also shows the stack trace of how the error was reached, including calls through other scripts. The same trace is printed when the standalone player hits a script error. Engine errors keep a readable message. A syntax error still names the script and line.

### Acceptance criteria

- [x] A runtime error in Output shows the script, the line, and the stack of script calls that led there.
- [x] Clicking the error still opens that script at that line.
- [x] An error raised through a call from another script names both the failing line and the caller.
- [x] The standalone player prints the same trace, and other scripts keep running.

---

## Phase 4: Breakpoints

**User stories**: 7

### What to build

During Play, the creator sets a breakpoint on a line in the script editor. When that line is about to run, Play pauses, the creator can inspect that script's locals, and continue resumes the game. Further breakpoints pause again. Removing a breakpoint or stopping Play clears the pause. Exported games have no breakpoints, and scripts have no way to turn the debugger on. If a pause cannot be done without giving exported games a way out of the sandbox, this phase stops: stack traces from Phase 3 remain, and breakpoints are left unimplemented rather than shipped unsafely.

### Acceptance criteria

- [x] A breakpoint pauses Play on that line before it runs, in the editor only.
- [x] While paused, the creator can see that script's locals, and continue runs on until the next breakpoint or the end of Play.
- [x] Clearing the breakpoint, or pressing Stop, resumes or ends Play without leaving the game stuck.
- [x] The exported player never pauses on a breakpoint, and a script cannot enable the debugger.
- [x] If a safe pause cannot be built, the phase records that, stack traces still show in Output, and no breakpoint control is exposed.

---

## Phase 5: Profiler

**User stories**: 8

### What to build

A profiler panel during Play reports CPU, GPU, memory, rendering, physics, and audio for the running game. The numbers come from measurements the frame loop already passes through, shown in the editor. The panel updates while Play runs and clears or freezes when Play stops. The exported game has no profiler panel. Turning the panel off does not change gameplay.

### Acceptance criteria

- [x] During Play the panel shows CPU time, GPU time, and memory use.
- [x] The panel shows rendering, physics, and audio cost for the same session.
- [x] The numbers change when the scene gets heavier (more parts, lights, sounds, or physics) and drop back when that load is removed.
- [x] Stop hides or freezes the live readings, and an exported game has no profiler.

---

## Phase 6: Visual debugging

**User stories**: 9

### What to build

The editor viewport gains toggles for physics colliders, raycasts, bounds, and audio emitters. Each toggle draws that information over the world in edit mode and during Play, and drawing it does not move objects or change collisions, ray results, or sound. Navigation paths are not in this phase; they arrive with agents. Overlays are absent from exported games.

### Acceptance criteria

- [x] Collider, raycast, bounds, and audio-emitter overlays can each be turned on and off.
- [x] A collider outline matches the shape the part actually collides with, including a part that does not collide and a terrain chunk.
- [x] A script raycast, while the raycast overlay is on, is visible for that cast, and an emitter marker sits on the sound's source.
- [x] Turning overlays on does not change physics, ray results, or audio, and an exported game does not draw them.

---

## Phase 7: Asset validation

**User stories**: 10

### What to build

One editor action scans the open project and reports missing files, invalid references, import failures, and broken dependencies in a single list. A missing model, a texture path that points nowhere, a prefab or controller that names a missing clip, and an asset that failed to import all appear there, each with where it was referenced. The scan does not delete or rewrite files. A damaged file still must not stop the rest of the project from opening; this list is how those problems are seen together.

### Acceptance criteria

- [x] The scan lists a missing file, an invalid reference, a failed import, and a broken dependency, each pointing at the owner of the reference.
- [x] A project with none of those problems reports a clear empty result.
- [x] Running the scan does not modify the project, and a missing file still leaves the rest of the project open.

---

# Milestone 2: Terrain foliage

## Phase 8: Foliage painting

**User stories**: 14, 13

### What to build

The terrain brush paints foliage as well as height and materials. The creator picks a plant or a prop, sets a density, and paints. Painted foliage appears on the terrain in the editor and in Play, follows the surface, and saves with the terrain. Undo treats a foliage stroke as one step. Sculpting, material painting, layer blending, and terrain collision keep working, and foliage does not collide yet.

### Acceptance criteria

- [x] Painting with a plant and with a prop scatters them on the stroked area at the chosen density, in the editor and in Play.
- [x] Foliage sits on the terrain surface, survives save and reload, and one undo removes the stroke.
- [x] Sculpting, material paint, and terrain collision still behave as they do today on a terrain that also has foliage.

---

## Phase 9: Foliage collision

**User stories**: 14

### What to build

Each foliage paint chooses whether those plants or props collide. Colliding foliage blocks parts the way other colliders do. Foliage left visual does not. The choice saves with the paint. Ground collision from the terrain itself is unchanged.

### Acceptance criteria

- [x] A part falls onto colliding foliage and comes to rest on it.
- [x] The same part falls through foliage marked visual and lands on the terrain.
- [x] The collide choice survives save and reload, and turning it off is one undo step.

---

# Milestone 3: Documentation

## Phase 10: Searchable API reference

**User stories**: 15

### What to build

A searchable API reference is generated from the engine's Lua definitions, covering classes, properties, events, and functions. In the editor the creator searches by name and opens an entry that shows what it does and how it is called. Regenerating after an API change updates the reference. The existing sample projects stay runnable.

### Acceptance criteria

- [ ] Searching for a class, a property, and a function each opens the matching entry.
- [ ] An entry describes the call and its arguments in the words the definitions already use.
- [ ] A new Lua API added with its definition appears in the reference after regeneration.

---

## Phase 11: Tutorials

**User stories**: 15

### What to build

Short tutorials, each one task, live with the editor's Learn page: place and move a part, run a script, press Play, and open a sample. Each tutorial points at a working sample project that already exists. The samples still run on their own.

### Acceptance criteria

- [ ] Learn lists the tutorials, and each one can be opened and followed without another document.
- [ ] Each tutorial names a sample project, and that sample still runs in the player and the editor.
- [x] The searchable API reference from Phase 10 still opens from the editor.

---

# Milestone 4: Character controllers

## Phase 12: Third-person walk and run

**User stories**: 16, 17, 19, 20

### What to build

A `ThirdPersonController` Instance, parented to a character model, walks and runs kinematically on flat ground. It is not a dynamic physics body, and dropping a crate still behaves as before. During Play its camera sits behind the character and follows it; the editor fly camera returns on Stop. Scripts and blocks set walk and run. The controller is inserted from the editor and saved with the world. There is no first-person controller, jump, crouch, or custom mode yet.

### Acceptance criteria

- [ ] During Play, walk and run move the character across a flat floor at different speeds, and the character does not tip over or fall as a physics body.
- [ ] The third-person camera follows behind the character during Play and is gone after Stop, with the fly camera back.
- [x] An unanchored crate still falls and collides, and editing the controller's speeds does not change crate gravity or friction.
- [x] The controller saves, reloads, and can be switched between walk and run from Lua and from blocks.

---

## Phase 13: Jump, slopes, and steps

**User stories**: 16, 20

### What to build

The third-person controller jumps, walks up and down slopes within a limit, and steps onto low obstacles. A slope steeper than the limit stops the character instead of launching it. A step taller than the limit blocks the character. Jump height and the slope and step limits are properties. Physics parts are unchanged.

### Acceptance criteria

- [ ] Jump rises and lands on the floor, and a second jump in the air does nothing.
- [ ] The character walks up a slope under the limit and up a step under the limit, and is stopped by a steeper slope and a taller step.
- [ ] Changing those limits changes what the character can cross, and a falling crate is unaffected.

---

## Phase 14: Crouch

**User stories**: 16

### What to build

Crouch is a built-in mode on the third-person controller. The character's collision height and the camera drop while crouched and restore when standing. The character cannot stand while something overhead would intersect the standing height, and stays crouched until the way is clear. Scripts and blocks set crouch. Walk and run still work while crouched, at a slower speed.

### Acceptance criteria

- [ ] Crouching lowers the character and the camera, and standing restores both.
- [ ] Under a low ceiling, the request to stand is refused until the character is clear.
- [ ] Crouched movement is slower than walking, and Lua and blocks can enter and leave crouch.

---

## Phase 15: First-person controller

**User stories**: 17, 19

### What to build

A `FirstPersonController` is a separate Instance class, not a mode of the third-person controller. It walks, runs, jumps, handles slopes and steps, and crouches with its own properties. During Play its camera is at the character's eyes and looks with the player's view. A character can have one or the other. The third-person controller is unchanged.

### Acceptance criteria

- [ ] A first-person controller walks, runs, jumps, crouches, and crosses the same slopes and steps as the third-person controller.
- [ ] Its camera stays at the eyes during Play, and Stop restores the fly camera.
- [ ] Inserting a first-person controller does not turn an existing third-person controller into one, and each saves under its own class.

---

## Phase 16: Custom movement modes

**User stories**: 18

### What to build

Either controller accepts a named custom movement mode. While that mode is active, the built-in walk, run, and crouch motion does not run; scripts and blocks supply the motion each frame. Clearing the mode returns to the built-in movement that was active before. The mode name is readable. Jump, slopes, and steps keep working in the built-in modes.

### Acceptance criteria

- [ ] Setting a custom mode stops built-in walking, and motion supplied by a script moves the character.
- [ ] Blocks can set and clear the same mode, and the current mode name can be read back.
- [ ] Clearing the mode restores walk, run, or crouch, including jump and step handling.

---

# Milestone 5: Navigation and AI

## Phase 17: Navigation mesh

**User stories**: 21

### What to build

The world builds one navigation mesh from collidable parts and terrain. A part that does not collide, and foliage marked visual, leave no walkable surface. Rebuilding after a move or a terrain edit updates the mesh. The editor can show the mesh. Nothing walks on it yet.

### Acceptance criteria

- [ ] A floor of parts and a terrain surface both produce walkable mesh, and a gap or a steep drop is not walkable.
- [ ] A non-colliding part and visual foliage do not add walkable surface.
- [ ] Moving a floor part or sculpting terrain, then rebuilding, changes the mesh to match.

---

## Phase 18: Agents and pathfinding

**User stories**: 22, 23, 26, 27

### What to build

A `NavigationAgent` Instance finds a path on the mesh to a destination and walks it. Scripts and blocks set that destination, or set a velocity to move the agent without a path. The editor inserts an agent and can draw its current path with the visual-debug path toggle. An unreachable destination does not move the agent and is reported. Obstacle avoidance beyond following the path is the next phase.

### Acceptance criteria

- [ ] An agent placed in the editor walks along the mesh to a destination set in properties, from Lua, and from blocks.
- [ ] A destination off the mesh does not move the agent, and the failure is visible.
- [ ] Setting a velocity moves the agent without a path, and the path toggle draws the path of an agent that has one.
- [ ] The path overlay does not change the route the agent takes.

---

## Phase 19: Obstacle avoidance

**User stories**: 22

### What to build

While an agent follows a path, it steers around obstacles that were not in the mesh when the path was built, such as a part that moved into the way. It returns to the path when the way is clear. A blocked path that cannot be steered around stops the agent and is reported. Path drawing still shows the path it is following.

### Acceptance criteria

- [ ] An agent following a path steers around a part pushed into that path and rejoins the path afterward.
- [ ] An agent faced with a blockage it cannot steer around stops and reports that it is blocked.
- [ ] With nothing in the way, the agent still follows the same path as in Phase 18.

---

## Phase 20: Perception

**User stories**: 24

### What to build

A navigation agent can see and hear. Sight has a range and an angle, and is blocked by colliders. Hearing has a range and is not blocked by walls. A creator sets those distances. When the player character enters sight or hearing, the agent reports which sense fired. Ranges of zero sense nothing.

### Acceptance criteria

- [ ] An agent reports the player character that walks into its view, and does not report one hidden behind a collider or outside the angle.
- [ ] An agent reports the player character that makes a sound inside hearing range, including one behind a wall.
- [ ] Sight range, sight angle, and hearing range save with the agent and change what it notices.

---

## Phase 21: Behavior trees

**User stories**: 25

### What to build

A behavior tree is its own asset, edited as a graph in the editor, and referenced by a navigation agent. The tree can test perception, set a destination, and wait. An agent with a tree runs it during Play. An agent without a tree still moves when scripts set a destination. Animator controllers are unchanged and cannot be used as the AI graph.

### Acceptance criteria

- [ ] A creator can add, connect, and save nodes that test sight and set a destination, and an agent using that tree chases when it sees the player.
- [ ] An agent with no tree still pathfinds when a script sets its destination.
- [ ] Opening an animator controller still edits animation states, and assigning one to an agent does not run it as AI.

---

# Milestone 6: Save and load

## Phase 22: Save slots and checkpoints

**User stories**: 28, 29, 31, 32

### What to build

A `Checkpoint` Instance marks a place in the world. Scripts and blocks write a numbered save slot that stores player data, world state, the active checkpoint, and settings, then load that slot later and restore them. Restoring a checkpoint puts the character back at that checkpoint with the captured state. The file lives in the player's data for that game, not in the project folder. Saving during editor Play does not modify the project, and Stop still restores the editing world. A missing slot fails clearly.

### Acceptance criteria

- [ ] Saving a slot and loading it restores player data, moved world state, the active checkpoint, and settings.
- [ ] Restoring a checkpoint places the character there with the state captured for it.
- [ ] The save file is outside the project folder, the project files are unchanged after a save, and Stop still restores the pre-Play world.
- [ ] Lua and blocks can save, load, and restore a checkpoint, and loading a missing slot fails without touching the current world.

---

## Phase 23: Save migration

**User stories**: 30

### What to build

Save files carry a version. Loading a slot from the previous version migrates it and then restores it. A save newer than the game understands is refused and does not overwrite the slot. This phase includes one real migration from the previous save version. Project migration from Phase 2 is a different path and stays as it is.

### Acceptance criteria

- [ ] A slot written by the previous save version loads, migrates, and restores the same player, world, checkpoint, and settings.
- [ ] A slot newer than this game understands is refused, and the existing slot file is unchanged.
- [ ] A current slot still round-trips without a migration step, and project folders are not used as save files.

---

# Milestone 7: UI runtime

## Phase 24: Keyboard and controller navigation

**User stories**: 34, 33

### What to build

Existing menus and HUDs can be moved through with the keyboard and a controller. Focus moves in a documented order, the focused control is visible, and the confirm button activates it the way a click does. A mouse click still works. Text boxes take keyboard input while focused and hand focus back when finished. HUDs, clicks, text, and layout scaling are unchanged for a layout that never uses focus.

### Acceptance criteria

- [ ] Keyboard and controller both move focus through a menu in the same order and activate the focused button.
- [ ] A focused text box receives typing, and leaving it fires the same finish event a click-away already fires.
- [ ] A HUD that is not navigated still shows, clicks, and scales as it does today.

---

## Phase 25: Localization

**User stories**: 35

### What to build

A project has a string table the creator can add entries to, with a value per language. UI text can refer to an entry. The game shows the value for the active language. A missing entry shows the source text that was authored on the control. Changing the active language updates the visible text. The table saves with the project as its own file and does not bump the project format version.

### Acceptance criteria

- [ ] A label bound to a table entry shows that entry's text in the active language, in the editor preview and in Play.
- [ ] Switching the active language updates every bound control.
- [ ] A missing entry shows the control's source text, and the table survives save and reload.

---

## Phase 26: Game UI accessibility

**User stories**: 36

### What to build

Game UI settings, separate from the editor's appearance settings, control text scale, a higher-contrast option, and reduced motion. They apply to HUDs and menus in Play and in the editor preview, and they are part of the settings a save slot stores. Each control has a name that navigation and scripts can read. Reduced motion turns off UI motion that is only cosmetic. Contrast and scale do not move world geometry.

### Acceptance criteria

- [ ] Raising text scale, turning on higher contrast, and turning on reduced motion each change the game UI and leave the 3D world as it was.
- [ ] Those settings persist in a save slot and restore on load, and they do not change the editor chrome theme.
- [ ] Each control's name can be read by a script, and keyboard focus still moves in the Phase 24 order.
