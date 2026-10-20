# PRD: Axcenity Engine v2

v1 is the shipped editor and single-player runtime: a Studio-like world, Lua and blocks, Play and Stop, animation, terrain, UI, audio, and desktop export. This document is what v2 adds, and which v1 behavior v2 must keep. It does not replace the v1 PRD.

## Problem Statement

v1 lets a creator build a world, script it, and export a desktop game. A finished game still needs more than a world. Characters need to walk, run, and jump without becoming a pile of physics tricks. Enemies and NPCs need to find their way around. Players need to save progress and come back. When something breaks or runs slowly, the creator needs a debugger, a profiler, and a way to see colliders, paths, and sounds in the world.

The editor tools that already work have to stay reliable while those systems arrive. Undo has to cover asset edits as well as the scene. Older projects have to open in a newer engine. The in-game UI has to work as a real game interface, with keyboard and controller navigation, translated text, and accessibility, in addition to the panels used to build it.

## Solution

**Axcenity Engine v2** keeps the v1 editor and runtime, and adds the systems a single-player game needs around them.

From the creator's point of view:

- Scene and asset editing stay on one command history, including clips, controllers, prefabs, and terrain. Saving still does not throw that history away.
- Opening an older project upgrades it. A copy of the previous files is kept. A project from a newer engine still asks the creator to update Axcenity.
- During Play, Output shows logs, errors, and script stack traces. The creator can set breakpoints, pause a script, inspect it, and continue. Exported games do not include the debugger.
- A profiler shows CPU, GPU, memory, rendering, physics, and audio while the game runs in the editor.
- Visual debugging draws physics colliders, raycasts, bounds, navigation paths, and audio emitters.
- An asset check reports missing files, invalid references, import failures, and broken dependencies in one place.
- The world editor, animation authoring, and terrain tools stay as they are. Terrain gains foliage: plants and props painted on the surface, with density and optional collision.
- A searchable API reference, short tutorials, and the existing sample projects explain the engine.
- First-person and third-person characters move with their own controllers: walk, run, jump, slopes, steps, crouch, and custom movement modes. Those controllers are separate from the general physics of parts.
- Navigation meshes, pathfinding, obstacle avoidance, agents, perception, and behavior trees drive enemies and NPCs. Scripts and blocks can drive an agent directly.
- Games save player data, world state, checkpoints, and settings into versioned save slots. Those saves are separate from the editor's project files.
- The existing UI toolkit runs game HUDs and menus, and also handles keyboard and controller navigation, localization, and accessibility.

Graphics, scripting, export, and licensing stay as decided for v1.

## Carried from v1

These already ship. v2 must not regress them. Gaps called out here are specified in the user stories below.

- **Undo and redo.** Scene edits, terrain strokes, and property-clip edits are command-history steps, and the history survives saving. Some asset actions, such as deleting an animation clip, are still not undoable.
- **Project manager.** New Project from a template, Open, recent projects, Project Settings, and version history. The project format records a version and refuses a newer file. It does not yet upgrade an older one.
- **World editor.** Selection, snapping, transform gizmos, grouping into Models and Folders, duplication, and multi-selection.
- **Animation authoring.** A timeline, property keyframes, animation events, animator state machines, transition blending, and 1D blend trees. Clips can be previewed without pressing Play. Hand-authored skeletal keyframes are not part of this.
- **Terrain.** Sculpting, painting, layer blending, generation, erosion, and terrain collision. Foliage is not included yet.
- **Debugger, in part.** The Output panel shows logs and script errors with the script name and line, and clicking an error opens that line. During Play the Explorer and Properties show the live world. Stack traces are omitted, and there are no breakpoints.
- **UI runtime, in part.** Screen GUIs, frames, text, buttons, images, text boxes, pages, and tables; click and text events; layout that scales to the screen. Keyboard and controller navigation, localization, and accessibility for the game UI are not included. Accessibility settings in the editor chrome do not cover the game.
- **Documentation, in part.** Engine docs, a Learn page in the editor, API definitions for an external editor, and working sample projects. There is no searchable API reference and no longer tutorials.

## User Stories

### Editor tools

1. As a creator, I want every scene edit and every asset edit (clips, animator controllers, prefabs, and terrain) to be one undo step, so that a mistake in the library is as cheap to undo as a mistake in the viewport.
2. As a creator, I want undo history to survive saving, so that saving is never the moment I lose the ability to go back.
3. As a creator, I want to create a project from a template, open an existing one, and change its settings, so that starting and configuring a game stays in the editor.
4. As a creator, I want an older project to open in a newer engine and be upgraded for me, with a copy of the previous files kept, so that an engine update never strands a game.
5. As a creator, I want a project written by a newer engine to tell me to update Axcenity, so that I do not silently damage it by saving with an older editor.
6. As a creator, I want Output to show logs, errors, and the script stack trace, so that I can see how a failure was reached.
7. As a creator, I want to set breakpoints, pause a script during Play, inspect that script, and continue, so that I can debug gameplay without guessing.
8. As a creator, I want a profiler during Play for CPU, GPU, memory, rendering, physics, and audio, so that I can see which part of the game is expensive.
9. As a creator, I want to draw physics colliders, raycasts, bounds, navigation paths, and audio emitters in the viewport, so that I can see what the game thinks is there.
10. As a creator, I want one asset check that lists missing files, invalid references, import failures, and broken dependencies, so that I can fix a project before I export it.
11. As a creator, I want selection, snapping, transform gizmos, grouping, duplication, and multi-selection to keep working as they do now, so that building worlds does not get worse as new tools arrive.
12. As a creator, I want the animation timeline, property keyframes, animation events, state machines, and blending to keep working as they do now, so that characters I already animated stay editable.
13. As a creator, I want to keep sculpting terrain, painting its layers, and colliding with it, so that worlds I already built still load and play.
14. As a creator, I want to paint foliage (plants and props) onto terrain, with density and a choice of whether it collides, so that a landscape can have grass, bushes, and rocks without placing each one.
15. As a creator, I want a searchable API reference, short tutorials, and working sample projects, so that I can look up a call and then see it running.

### Character controllers

16. As a creator, I want a character to walk, run, jump, handle slopes, step up small obstacles, and crouch, so that movement feels like a game and not like a sliding crate.
17. As a creator, I want a first-person controller and a third-person controller as separate choices, so that I pick the game I am making instead of switching a hidden mode.
18. As a creator, I want custom movement modes I can name and switch from scripts and blocks, so that a swim, climb, or vehicle can replace walking without rewriting physics.
19. As a creator, I want each controller to own its camera, so that first-person and third-person framing stay with the character.
20. As a creator, I want character movement kept separate from the general physics of parts, so that tuning a jump never changes how crates fall.

### Navigation and AI

21. As a creator, I want a navigation mesh built from parts and terrain, so that characters know where they can walk.
22. As a creator, I want pathfinding and obstacle avoidance, so that an agent can cross the world and steer around things in the way.
23. As a creator, I want navigation agents I can place and script, so that enemies and NPCs share one way of moving.
24. As a creator, I want simple perception, sight and hearing, so that an agent can notice the player without me writing the geometry myself.
25. As a creator, I want behavior trees for AI decisions, so that enemy and NPC logic is visible and editable.
26. As a creator, I want scripts and blocks to be able to move an agent directly, so that a one-off behavior does not require a tree.
27. As a creator, I want navigation paths drawn by the visual debugging tools, so that I can see why an agent took the route it did.

### Save and load

28. As a player, I want my progress kept in save slots, so that I can leave and come back to the same game.
29. As a creator, I want a save to hold player data, world state, checkpoints, and settings, so that one slot restores the game I meant to store.
30. As a creator, I want save files versioned and upgraded when the game changes, so that an update does not wipe old slots.
31. As a creator, I want game saves kept separate from the editor project, so that playing never rewrites the world I am editing.
32. As a creator, I want checkpoints I can place and restore, so that a fall or a death can return the player to a chosen spot.

### UI runtime

33. As a creator, I want the existing UI objects to keep working as HUDs and menus, with clicks, text, and layout that scales to the screen, so that interfaces I already built stay valid.
34. As a player, I want to move through menus with the keyboard and a controller, so that a menu is usable without a mouse.
35. As a creator, I want my game's text in a string table I can translate, so that the same UI can ship in more than one language.
36. As a player, I want the game UI to respect text scale, contrast, reduced motion, and a name on each control, so that the interface stays usable when I need those options.

## Implementation Decisions

### Relationship to v1

- v2 is an extension of the shipped engine. The v1 PRD and its plan stay the record of what already shipped.
- New systems follow the same rules: Instances in the arena, one Lua runtime, blocks that translate to Lua, a player that contains no editor, and a renderer that only draws.
- Editor-only tools (breakpoints, the profiler, visual debugging, asset validation) run in the editor. Exported games do not ship them.

### Command history

- Scene edits and asset edits share one history. A clip, controller, prefab, or terrain change is one step, including deletes that today are permanent.
- A drag, a stroke, or a typed value remains a single step. Saving does not clear the history.

### Projects

- Creating, opening, and configuring a project stay as they are: templates, recent projects, and project settings.
- When the project format version increases, opening an older project migrates it in place and keeps a copy of the previous files. A newer version than the engine understands still refuses to open and tells the creator to update.

### Debugger

- Output keeps logs and `script:line` errors, and adds the stack trace that is currently omitted.
- Breakpoints are an editor feature during Play. The creator sets them in the script editor, the game pauses on that line, and the creator can inspect the script and continue.
- Exported games stay sandboxed. Scripts cannot turn the debugger on, and the player build has no breakpoint support.

### Profiler

- The profiler is a panel available during Play. It reports CPU, GPU, memory, rendering, physics, and audio for the running game.
- It is an editor view over measurements the runtime already takes. It is not a second simulation.

### Visual debugging

- Each overlay is a toggle: physics colliders, raycasts, bounds, navigation paths, and audio emitters.
- Overlays draw in the editor viewport. They do not change gameplay, and they are absent from exported games.

### Asset validation

- One action scans the open project and reports missing files, invalid references, import failures, and broken dependencies together.
- A missing or damaged file still must not prevent the rest of the project from opening. The report is how the creator sees those problems in one list.

### World editor and animation

- Selection, snapping, gizmos, grouping, duplication, and multi-selection stay as shipped.
- Animation stays the Animation Studio: timeline, property keyframes, events, state machines, transition blends, and 1D blend trees. Skeletal clips remain imported takes, trimmed and previewed, rather than keys posed by hand.

### Terrain foliage

- Foliage is painted onto terrain the way materials are: a brush, a density, and a choice of plants or props.
- Foliage can collide or stay visual. Terrain collision for the ground itself stays as it is.

### Documentation

- The API reference is generated from the engine's Lua definitions and is searchable.
- Tutorials are short and task-shaped. The existing sample projects remain the working examples those tutorials point at.

### Character controllers

- A character controller moves a character kinematically. It queries the world for ground, slopes, and steps. It does not turn the character into a dynamic physics body.
- First-person and third-person are separate controller types. Each owns its camera.
- Built-in movement covers walking, running, jumping, slopes, step handling, and crouching. A custom movement mode is a named mode the creator switches to from Lua or blocks; while it is active, that mode supplies the motion.
- The physics system continues to simulate parts, collisions, and terrain. Controller code does not live inside it.

### Navigation and AI

- A navigation mesh is built from collidable parts and terrain. Agents pathfind on that mesh and avoid obstacles.
- A navigation agent is an Instance the creator places and scripts. Perception is sight and hearing with distances the creator sets.
- Decisions are behavior trees, edited as their own graph. Animator state machines stay the animation system and are not reused as AI graphs.
- Lua and blocks can set an agent's destination or velocity directly, so a simple NPC does not need a tree.
- Path drawing uses the navigation-path overlay from visual debugging.

### Save and load

- A save slot stores player data, world state, the active checkpoint, and settings.
- Save files carry a version and are migrated when that version changes.
- Saves live with the player's data for that game. They are never written into the project the creator edits. Play and Stop in the editor keep restoring the editing world as they do today.
- A checkpoint is a place the creator marks. Restoring it puts the player back there and applies the player and world state captured for it.

### UI runtime

- HUDs, menus, interaction events, and layout scaling stay on the existing UI Instances and stylesheets.
- Focus navigation moves between controls with the keyboard and a controller, in a documented order, and activates the focused control.
- Localization reads a project string table. UI text can refer to an entry; a missing entry shows the source text.
- Accessibility for the game UI is text scale, a higher-contrast option, reduced motion, and a name on each control that navigation and scripts can read. These are game settings, separate from the editor's own appearance settings.

## Out of Scope

- **Multiplayer networking**, an online platform, accounts, and a marketplace. v2 stays single-player, with the same server and client script split as v1.
- **Web and mobile** export.
- **Hand-authored skeletal animation**, inverse kinematics, retargeting between skeletons, morph targets, and GPU skinning. Property keyframes and imported skeletal clips remain the animation tools.
- **Custom avatars.** Character controllers move a character the creator built. They do not add a body-customization system.
- **Replacing the physics engine.** Controllers and navigation use it. They do not reimplement rigid bodies, joints, or terrain collision.
- **A player-facing debugger or profiler.** Breakpoints, the profiler, visual debugging overlays, and asset validation are editor tools.
- **Markup-based UI, full CSS, and an embedded web browser.** Those remain out of scope as in v1.
- **Anything already shipped in v1**, except where a story above extends it. v2 does not rebuild the renderer, the scripting sandbox, or export.

## Further Notes

- v2 stories are the backlog for a later plan. This document does not by itself schedule or implement them.
- The largest new risks are the character controller (it has to feel solid on slopes and steps) and navigation (mesh builds have to match the collision the player actually hits). Both stay outside the physics crate's own simulation.
- Behavior trees are the AI graph so they are not confused with animator state machines. A later pass can add state-machine AI only if trees prove a poor fit; that is not a v2 requirement.
- Breakpoints depend on pausing one script during Play without giving exported games a way to inspect or escape the sandbox. If that cannot be done safely, logs, errors, stack traces, and live inspection still ship, and breakpoints wait.
- Save files and project files must stay different formats. A save is allowed to reference project assets. It is not allowed to be a second copy of the project.
