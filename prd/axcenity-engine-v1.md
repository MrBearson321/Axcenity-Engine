# PRD: Axcenity Engine v1

## Problem Statement

People who want to make 3D games (especially those who learned on Roblox Studio) face an uncomfortable choice. Roblox Studio is easy and fun: place parts, attach a script, press Play. But games are locked to the Roblox platform, and creators don't own a standalone game they can ship. General-purpose engines like Unity, Unreal and Godot give that freedom, but they're intimidating for beginners. They use unfamiliar concepts, heavier languages, and have no gentle on-ramp for people who can't code yet.

There is no lightweight, cross-platform, open-source engine that combines:

- the Roblox Studio workflow and mental model (a tree of objects, Properties panel, Play button),
- Lua scripting that feels familiar to Roblox developers,
- a visual, block-based way to script for people who don't code yet, with a clear path to real code,
- full ownership of the finished game as a standalone desktop app.

## Solution

**Axcenity Engine** is a free, open-source 3D game engine and editor, written in Rust, that runs on Windows, macOS and Linux.

From the creator's point of view:

- They open the Axcenity editor and see a Roblox Studio-like layout: a 3D viewport, an Explorer (tree of objects), a Properties panel and an Output panel.
- They build worlds by placing parts (blocks, spheres, cylinders, wedges) and imported 3D models, then moving, scaling and rotating them.
- They add behavior with **Lua 5.4 scripts** using a Roblox-familiar API (`Instance.new`, `.Parent`, `:Connect`, `Vector3`, `task.wait`), or with **Scratch-style blocks** that translate to Lua. A "View as Lua" button shows the code each block script produces.
- They press **Play** to test instantly inside the editor, and **Stop** to return to their untouched world.
- They build in-game UI (health bars, menus) as objects in the same tree, styled with **CSS** and laid out with flexbox, similar to s&box.
- They **export** a standalone single-player game for Windows, macOS and Linux from whichever OS they use.
- They own their game completely. The engine is Apache-2.0 licensed, and the only requirement is credit, which export handles automatically.

Graphics aim for a modern, realistic look (physically based materials, shadows, many lights, post-processing), delivered in stages so the editor and scripting are never blocked waiting for the renderer.

## User Stories

### Building worlds

1. As a creator, I want to open a new project and see a 3D viewport with a default floor and sky, so that I can start building immediately.
2. As a creator, I want to insert primitive parts (block, sphere, cylinder, wedge), so that I can build worlds without external 3D tools.
3. As a creator, I want to import 3D models in the glTF format, so that I can use art made in Blender or downloaded from asset sites.
4. As a creator, I want move, scale and rotate tools with grid snapping, so that I can place objects precisely.
5. As a creator, I want to select objects by clicking them in the viewport or in the Explorer, so that I can find and edit things easily.
6. As a creator, I want an Explorer panel showing every object in a tree (game → Workspace → Model → Part), so that I understand how my game is organized.
7. As a creator, I want to rename objects and reparent them by dragging in the Explorer, so that I can organize my world.
8. As a creator, I want a Properties panel that shows and edits the selected object's properties (Name, Position, Size, Color, Material, Anchored, and so on), so that I can change things without code.
9. As a creator, I want to group objects into Models and Folders, so that I can keep complex worlds tidy.
10. As a creator, I want parts to have realistic material properties (color, metalness, roughness) and presets like plastic, metal, glass and neon, so that my world looks good.
11. As a creator, I want to add lights (sun, point lights, spot lights) and adjust them, so that I can set the mood of my scenes.
12. As a creator, I want to mark parts as anchored or unanchored, so that I control which parts fall and collide under physics.
13. As a creator, I want undo and redo in the editor, so that mistakes are cheap.
61. As a creator, I want to choose which objects to import from a model file that holds many, and whether they arrive grouped as laid out or as separate parts, so that I can use one prop from a large pack.
62. As a creator, I want imported models to use all their standard texture maps (base color, normal, roughness or glossiness, metalness or specular, emission, opacity, ambient occlusion), so that they look as their artist made them.
63. As a creator, I want FBX files with embedded or channel-packed textures to import with the right maps worked out for me, so that assets from stores and other engines just work.
64. As a creator, I want to see and change each imported material's texture maps, both while importing and afterwards, so that I can fix a wrong guess or retexture a model.
65. As a creator, I want an asset browser of my project's models, prefabs, images, textures, sounds and particle textures, plus a built-in set of common props, that I can drag straight into the world, so that I don't rebuild the same things over and over.
66. As a creator, I want to save any part or Model as a prefab and place copies of it from the asset browser, so that repeated props stay quick to reuse.
67. As a creator who learned with F3X Building Tools, I want familiar building tools (Move, Resize, Rotate, Paint, Material, Anchor, Collision, Lighting and Emission) that work on everything I've selected at once, so that I can build quickly the way I already know.
68. As a creator, I want an angular tool like Archimedes that keeps placing copies of a part at a set angle, a set number of times, so that I can build arches, loops, curved roads and round towers.
69. As a creator, I want particle emitters with built-in particle types (sparks, smoke, fire, stars, magic) and my own particle textures (such as Kenney's free Particle Pack), editable live in the editor and from scripts, so that my game has effects.

### Animating characters

70. As a creator, I want to import FBX characters with their skeleton and animation clips, and see what the file holds (model, skeleton, clips), so that animated assets from Blender, Mixamo-style rigs or asset stores just work.
71. As a creator, I want to split one long take into several named clips by frame range, while importing or later, so that a file with every move on one timeline becomes a usable set of clips.
72. As a creator, I want an Animation Library where I can preview, organise and store clips and set each one's playback speed, looping, root motion and animation events, so that clips behave right before I use them.
73. As a creator, I want an animation state machine with a default (idle) state and transitions between states, so that my character switches between idle, walk, jump and attack as the game changes.
74. As a creator, I want transitions with conditions, a blend duration, exit time, interruption and priority, so that changes between animations look smooth and happen exactly when they should.
75. As a creator, I want Bool, Trigger and Number parameters on an animator, set by scripts and blocks, so that gameplay (IsGrounded, Jump, Speed) drives the animations.
76. As a creator, I want to preview an animator in its own window (changing parameters, firing triggers and scrubbing blends) without playing the game, so that I can tune animations quickly.
77. As a creator, I want 1D blend trees that mix related motions by a number (Idle, Walk and Run by Speed), so that movement speeds up smoothly instead of popping between clips.
78. As a creator, I want a state to return to another state when its clip finishes or a condition holds, with the transitions made for me, so that one-shot moves such as attacks and jumps need no hand-wired exits.
79. As a creator who uses blocks, I want animation blocks (when a key is pressed, set a trigger, play a state), so that I can drive animations without writing Lua.

### Scripting with Lua

14. As a creator who codes, I want to add a `Script` (runs on the server side) or a `LocalScript` (runs on the player's side) to my game, so that my game logic is organized the same way Roblox's is.
15. As a Roblox developer, I want the Lua API to use familiar patterns like `Instance.new`, `.Parent`, `:Connect`, `Vector3.new`, `CFrame`-style transforms and `task.wait`, so that my existing knowledge carries over.
16. As a creator, I want to react to events like `Touched`, `Changed`, player input and button clicks, so that my game responds to what happens.
17. As a creator, I want `task.wait`, `task.spawn` and `task.delay`, so that I can write timed logic top-to-bottom without building timers by hand.
18. As a creator, I want `RemoteEvent` objects to send messages between Scripts and LocalScripts, so that my game is structured correctly for multiplayer later.
19. As a creator, I want my scripts saved as real `.lua` files, so that I can edit them in VS Code if I prefer.
20. As a creator using VS Code, I want autocomplete and error hints for the engine's API, so that I can write code faster with fewer mistakes.
21. As a creator, I want a script editor inside the Axcenity editor with syntax highlighting, so that I don't need any other tools.
22. As a creator, I want `print` output and script errors (with script name and line number) shown in an Output panel, so that I can debug my game.
23. As a creator, I want the editor to warn me when I use something on the wrong side (for example, reading input in a server `Script`), so that I learn the server/client split without confusion.

### Visual scripting (blocks)

24. As a beginner who can't code, I want to build scripts by snapping together Scratch-style blocks, so that I can make game logic without typing code.
25. As a beginner, I want block shapes that only fit where they're valid, so that I can't build broken scripts.
26. As a beginner, I want event blocks like "when this part is touched" and action blocks like "set color", "wait 1 second" and "move to", so that I can make common game mechanics.
27. As a beginner, I want each block script to have a clear server or client badge, so that I understand where it runs.
28. As a learner, I want a "View as Lua" button that shows the Lua my blocks produce, so that I can gradually learn to code.
29. As a creator, I want block scripts to have the same capabilities as the Lua API they're built on, so that choosing blocks never means losing features I need.
30. As a beginner, I want errors in my block scripts to highlight the block that caused them, so that I can fix problems without reading Lua.

### Playing and testing

31. As a creator, I want to press Play and immediately test my game inside the editor, so that iteration is fast.
32. As a creator, I want pressing Stop to restore my world exactly as it was before Play, so that testing never damages my work.
33. As a creator, I want to see live objects and properties change in the Explorer and Properties panels while the game runs, so that I can understand what my scripts are doing.
34. As a creator, I want my project autosaved before each Play, so that a crash never loses my work.
35. As a creator, I want a script with an infinite loop to be stopped with a clear error instead of freezing the editor, so that mistakes don't cost me my session.
36. As a creator, I want engine errors during Play to be reported instead of silently closing the editor whenever possible, so that I stay in control.

### Physics

37. As a creator, I want unanchored parts to fall, collide and rest realistically, so that my world feels physical.
38. As a creator, I want collision events (`Touched`) to fire reliably, so that I can build mechanics like doors, traps and pickups.

### Graphics

39. As a creator, I want realistic, physically based materials from the start, so that my game looks modern.
40. As a creator, I want sun shadows, many point and spot lights, sky and environment lighting, and post-processing (bloom, tone mapping) as the engine matures, so that my games can look polished.
41. As a creator, I want my scenes to keep working as graphics features are added, so that engine upgrades never require me to redo my work.

### Game UI

42. As a creator, I want to build in-game UI from objects in the tree (`ScreenGui`, `Frame`, `TextLabel`, `TextButton`, `ImageLabel`), so that UI works like everything else in the engine.
43. As a creator, I want to give UI objects a `Class` and an `Id` and style them with `.css` files, so that I can style my UI the way websites and s&box do.
44. As a creator, I want flexbox layout (rows, columns, alignment, gaps, padding, margin), so that my UI arranges itself cleanly at any screen size.
45. As a creator, I want CSS support for colors, borders, rounded corners, fonts, CSS variables and `:hover` / `:active` states, so that my UI looks good and feels responsive.
46. As a creator, I want to change UI from scripts and blocks (set text, toggle visibility, react to button clicks), so that my UI shows live game state.
47. As a creator, I want UI changes to show live in the editor viewport, so that I can design UI visually.

### Audio

48. As a creator, I want to play sound effects and music from scripts and blocks, so that my game has sound.

### Projects and saving

49. As a creator, I want my project saved as a folder of readable text files, so that I can back it up and use `git` for version control.
50. As a creator, I want script file names to show whether they're server or client scripts, so that my project is easy to navigate outside the editor.
51. As a creator, I want the editor to notice when I edit a script in an outside editor and reload it, so that VS Code and the Axcenity editor stay in sync.
52. As a creator, I want a damaged file to affect only that one file, not my whole project, so that corruption is recoverable.

### Exporting and sharing

53. As a creator, I want to export my game as a standalone app for Windows, macOS and Linux from whichever OS I use, so that I can share it anywhere.
54. As a creator, I want exported games to be packed into one compact game file next to the player program, so that players never see my project folder.
55. As a creator, I want export to automatically include the required license credits, so that I comply with the engine's license without thinking about it.
56. As a creator, I want an optional "Made with Axcenity" splash screen that's on by default and can be turned off, so that I can support the engine or hide branding.
57. As a creator, I want to fully own the games I make, with no royalties or fees, so that I can sell or share them freely.

### Contributors and forks

58. As a contributor, I want the engine split into focused crates with clear boundaries, so that I can understand and change one area without learning the whole engine.
59. As a forker, I want to be able to fork and modify the engine freely under Apache-2.0, so that I can adapt it to my needs, provided I keep the `NOTICE` credit and rename my fork.
60. As a developer, I want the editor and engine to build and run the same way on Windows, macOS and Linux, so that no platform is second-class.

## Implementation Decisions

### Scope and platform
- v1 is a **playable slice**: a desktop editor plus single-player runtime. Multiplayer, an online platform, accounts and a marketplace come later, but the design must not block them.
- The editor and exported games target **Windows, macOS and Linux only**. Web and mobile are out of scope, but runtime code avoids desktop-only assumptions. For example, all file access goes through one asset system so other platforms stay possible.

### Foundation
- The engine is written in **Rust** and assembled from focused libraries rather than built on an existing engine:
  - `winit` for windows and input
  - `wgpu` for graphics (Vulkan, Metal, DirectX 12)
  - `rapier` (3D) for physics
  - `egui` for the editor UI
  - `mlua` for Lua 5.4
  - `kira` for audio
  - `taffy` for UI layout
  - a CSS parser such as `cssparser` or `lightningcss`
  - a text library such as `cosmic-text` or `glyphon`
- Dependency versions are pinned and upgraded deliberately.

### Code organization
- The code is a **Cargo workspace of focused crates**, each with one job:
  - **core**: Instance tree, properties, events, arena storage
  - **render**: the `wgpu` renderer
  - **physics**: `rapier` integration
  - **script**: Lua runtime, sandbox, scheduler, engine API
  - **blocks**: block-script data model and translation to Lua
  - **ui**: game UI, including CSS parsing, layout and drawing
  - **audio**: `kira` integration
  - **project**: project folder loading and saving, export packing
  - **player**: the small runtime shipped with games
  - **editor**: the `egui` editor app
- The **player never depends on the editor**. Exported games contain no editor code.
- Boundaries are enforced through crate dependencies. The renderer depends only on what it needs to draw, not on scripting or the editor.

### Data model
- Everything in a game is an **Instance** in a single tree, Roblox-style. Each Instance has a class (`Part`, `Model`, `Folder`, `Script`, `LocalScript`, `RemoteEvent`, lights, UI classes, and so on), a `Name`, a `Parent`, children, typed properties and events.
- Instances are stored in an **arena** and refer to each other by generational IDs, not direct pointers. This avoids Rust ownership problems with parent/child links and keeps IDs safe after deletion.
- Top-level services follow the Roblox pattern (for example, `Workspace` for the 3D world and a lighting service for environment settings). The exact list is defined during implementation.

### Rendering
- The long-term goal is a **modern realistic look**, built on the engine's own `wgpu` renderer.
- Material data uses **physically based properties from day one** (base color, metalness, roughness, plus emissive and transparency for neon and glass), so later stages add to the renderer rather than redesigning it.
- The renderer sits behind a narrow boundary: it receives a description of what to draw (meshes, materials, transforms, lights, camera) and draws it. It knows nothing about Instances, scripts or the editor.
- Rendering is delivered in stages, and editor and scripting work begins after stage 1:
  1. PBR materials with basic lighting
  2. Sun shadows
  3. Many point and spot lights
  4. Sky and environment lighting
  5. Post-processing (bloom, tone mapping)
- Supported geometry: built-in primitives (block, sphere, cylinder, wedge) and imported glTF and FBX models, with their standard PBR texture maps.

### Scripting
- The language is **Lua 5.4** through `mlua`.
- **Sandboxing**: each script runs in a restricted environment. Dangerous standard libraries and functions (file and OS access, debug, raw code loading) are removed or replaced with safe engine versions.
- The API is **Roblox-familiar, not Roblox-compatible**. It uses the same core ideas and common names (`Instance.new`, `.Parent`, `:Connect`, `Vector3`, `task.*`), cleans up awkward parts, and makes no promise to match Roblox in every detail. Roblox's name, logos and assets are never used.
- **Execution model**: every script and event handler runs as a Lua coroutine managed by a task scheduler. `task.wait`, `task.spawn` and `task.delay` pause and resume coroutines without blocking the engine.
- **Order of work each frame** (fixed and documented):
  1. Read input
  2. Resume scripts whose wait is over and fire queued events
  3. Step physics at a fixed 60 Hz
  4. Fire collision and touch events
  5. Update UI layout
  6. Render
- **Server/client split from day one**: `Script` runs on the server side, `LocalScript` on the client side, and `RemoteEvent` carries messages between them. In v1, both sides run in one process connected by an **in-memory loopback "network,"** so multiplayer later means replacing the transport rather than redesigning the API.
- **Watchdog**: a Lua instruction-count hook stops runaway scripts (such as infinite loops) with a clear error instead of freezing the editor.
- **Editor tooling**: the engine generates a definitions file in Lua Language Server annotation format describing the whole API, enabling autocomplete and type hints in VS Code and similar editors.

### Visual scripting
- Visual scripts are **Scratch-style blocks** that snap together vertically. Block shapes encode what fits where, so invalid programs can't be built.
- Block scripts are **translated to Lua** before running. There is exactly one runtime for game logic, so every API feature is available to both Lua and blocks.
- Generated Lua is **readable**, viewable via "View as Lua," and **tagged with source block IDs**, so runtime errors and debug highlighting map back to the responsible block.
- Translation is one-way (blocks to Lua). Converting hand-written Lua into blocks is not supported.
- Each block script is either a server or a client script, shown with a badge.

### Game UI
- UI elements are **Instances** (`ScreenGui`, `Frame`, `TextLabel`, `TextButton`, `ImageLabel` to start), so they appear in the Explorer, save with the project, and are scriptable from Lua and blocks.
- UI Instances have `Class` and `Id` properties. Projects include **`.css` stylesheets** using a supported subset of CSS:
  - selectors: type, `.class`, `#id`, descendant, `:hover`, `:active`
  - flexbox layout
  - sizes, padding and margin
  - colors, borders, rounded corners and fonts
  - CSS variables
  - transitions in a later iteration
- Plain CSS is used, not SCSS. Layout is computed by `taffy`, and the engine's renderer draws UI.
- Design leaves room for a later **markup layer** (HTML-like UI files that generate Instances, with data binding), without redesigning the UI system.

### Editor and Play mode
- The editor is an `egui` application with a 3D viewport, Explorer, Properties, Output, a script editor, a block editor, and move, scale and rotate tools.
- **Play runs inside the editor on a copy of the world.** Stop discards the copy and restores the editing world untouched. While playing, the Explorer and Properties panels show the live game.
- Safety nets:
  - autosave before every Play
  - the script watchdog
  - catching Rust panics during Play where possible, and reporting them instead of crashing the editor
- A "Run standalone" option can be added later on top of export.

### Animation
- Inspired by Unity's Animator, kept small. **FBX** is the animation source: importing one reads its model, its **skeleton** (the bones its skin uses, and their parents) and its **takes**, which can be split into named **clips** by frame range.
- A clip is a readable file (`assets/animations/*.anim.json`) naming its FBX take and range, with its playback speed, looping, root motion and animation events. The **Animation Library** lists, previews and edits them.
- An **Animator** Instance inside a MeshPart plays an **animator controller** (`*.controller.json`): parameters (Bool: persistent state; Trigger: a one-time request, consumed when a transition uses it; Number: a continuous value), states (a clip, or a 1D blend tree of clips by a Number), a default state and transitions (conditions, blend duration, exit time, interruption, priority). A state can **return** to another when its clip finishes or a condition holds, which generates its transitions.
- Blending: level 1 is **transitions** (cross-fading the old pose into the new one over the blend duration); level 2 is **blend trees** (mixing related motions by a parameter, such as Speed).
- Gameplay drives animation through parameters: Input → Condition → Action → Animation. Lua (`animator:SetBool`, `SetNumber`, `SetTrigger`, `Play`) and blocks (an Animation category, which translates to the same Lua) set them; there is still one scripting runtime.
- Skinning runs on the CPU in v1 (`assets` poses a model; the renderer just redraws changed vertices), which is simple and plenty for a few characters. The **Animation Studio** panel in the editor holds the library, a state machine graph, an inspector and an interactive preview with its own camera, separate from Play.

### Project format
- A project is a **folder of readable text files**:
  - a project manifest
  - world and Instance data in a readable text format
  - every Lua script as its own `.lua` file, with server and client scripts distinguished in the file name
  - every block script as its own file
  - stylesheets as `.css` files
  - assets (models, textures, sounds) as ordinary files
- The editor **watches the project folder** and reloads files changed by outside tools.

### Export
- The editor ships with **pre-built player programs** for Windows, macOS and Linux, built automatically by continuous integration (such as GitHub Actions).
- Export **packs the project into one compact game file** and pairs it with the player for each chosen OS. Exporting for any OS works from any OS.
- Export automatically includes a **licenses and credits file** containing the engine's `NOTICE`.
- An optional **"Made with Axcenity" splash screen** is enabled by default and can be turned off per project.
- macOS exports are unsigned in v1. Players open them via right-click → Open. Code signing and notarization are deferred.

### Licensing
- The engine is licensed under **Apache-2.0** with a **`NOTICE` file** carrying the credit line "Built with Axcenity Engine, © 2026 Paul Janota and contributors."
- Games made with the engine belong entirely to their creators, with no royalties. The README states this clearly.
- Forks must keep the `NOTICE` and may not use the Axcenity name to imply they're the official project.

### Build order
- **Milestone 0, learning:** Rust fundamentals and introductory `wgpu`.
- **Milestone 1, end-to-end slice:** a window on all three OSes, lit PBR parts, one Lua script using `task.wait`, physics dropping a part, and loading from a project folder. No editor yet.
- **Milestone 2, minimal editor:** viewport, Explorer, Properties, select and move tools, Play/Stop, Output.
- **Milestone 3, scripting depth:** `Script`, `LocalScript`, `RemoteEvent`, events, sandbox, watchdog, Lua Language Server definitions.
- **Milestone 4, game UI:** Instances plus CSS with `taffy`.
- **Milestone 5, blocks editor:** blocks, translation to Lua, "View as Lua."
- **Milestone 6, export:** CI-built players, packing, credits file, splash screen.
- **Ongoing:** renderer stages 2–5, audio, and polish.

## Out of Scope

- **Multiplayer networking**: real servers, matchmaking and replication. Only the server/client script split and in-memory loopback are in v1.
- **Online platform**: accounts, game hosting, publishing, a marketplace or asset store.
- **Web and mobile** export targets.
- **Roblox API compatibility**: running arbitrary Roblox scripts unchanged is not a goal.
- **Other Lua versions**: Luau, LuaJIT, typed Lua or a second scripting language.
- **Node-graph visual scripting** (Blueprint-style), and converting Lua into blocks.
- **Markup-based UI** (Razor or HTML-style files with data binding), SCSS, full CSS compliance and embedded web browsers.
- **A standalone "Run" mode** separate from in-editor Play (can be added after export exists).
- **macOS code signing and notarization.**
- **Advanced rendering** beyond the five planned stages, such as ray tracing or global illumination.
- **Terrain, character controllers and avatars.** These weren't decided in this round and need their own design pass. (Particles and animation were added later; see stories 69–79.)
- **Animation beyond the Animation Studio's scope**: glTF animation, 2D blend trees, layers and avatar masks, inverse kinematics, retargeting between skeletons, morph targets (blend shapes) and GPU skinning.

## Further Notes

- **Name**: *Axcenity Engine*, pronounced *ax-SEN-ih-tee*. State the pronunciation in the README. Claim the name on crates.io, as a GitHub organization and as a domain early, and consider redirects for common misspellings (Ascenity, Accenity).
- **Biggest risk**: the realistic renderer is the largest single piece of work. Staging it, with PBR-ready data from day one, keeps it from blocking everything else.
- **Rust learning curve**: the primary developer is new to Rust. Milestone 0 is deliberate, and designs prefer simple, idiomatic patterns (arenas over shared pointers, clear crate boundaries) over clever ones.
- **Open questions for later design passes**:
  - the exact Instance class list and services
  - the world file text format (RON or JSON)
  - the packed game file format and extension
  - character and player controller design
  - the block palette
  - the editor's dockable layout
- **Trademark hygiene**: Roblox's API names and patterns may be echoed. Roblox's name, logos and assets may not.
