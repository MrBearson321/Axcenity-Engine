---
description: The programs that make up Axcenity, how a frame runs, the main crates and the rules that keep the engine simple.
icon: stack
---

# Engine architecture

Axcenity is written in Rust and scripted in Luau. One engine runs in four programs that share the same frame code.

## The programs

| Program | What it is |
|---|---|
| Editor (`axcenity-editor`) | Where you build games. **Play** runs the game inside the editor on a copy of the world, so Stop puts everything back. |
| Player (`axcenity-player`) | What players run: the game in a window, from a project folder or a built `game.axgame`. |
| Server (`axcenity-server`) | A dedicated multiplayer server with no window, graphics or audio. |
| Test runner (`axcenity-test`) | Runs a project's game tests (`tests/*.spec.lua`) with no window. |

## How a frame runs

Each frame takes the same steps, in the same order, in every program: receive network messages, move characters and navigation agents, read input, resume scripts, step physics (60 times a second, whatever the frame rate), fire `RunService.Heartbeat`, pose animations, build the list of things to draw, lay out the UI, draw, and send network messages. Programs leave out the steps they don't have: the server never draws. Sound plays on its own thread.

## The main parts

| Part | What it does |
|---|---|
| core | The **Instance tree**: every object, its properties, events, tags and attributes. See [Instances, properties and events](instances.md). |
| script | The Luau runtime, the sandbox, the scheduler and the engine API. |
| blocks-runtime | Turns block scripts, graphs, UI Event Graphs, behaviour trees and dialogues into Lua. |
| physics | Rigid bodies, joints, movers, vehicles, ragdolls and destruction (built on rapier). |
| anim | Clips, state machines, blending, IK and cutscene sequences. |
| render and scene | Turning the world into draw lists and drawing them with the GPU (wgpu: Vulkan, DirectX 12 or Metal). |
| audio | Sound playback, spatial sound, buses and effects (built on kira). |
| ui | Game UI: the CSS cascade, layout and widgets. |
| assets | Loading models, textures, materials, sounds and fonts. |
| project | Reading and writing projects, levels, prefabs, packages and built games. |
| net | Multiplayer: hosting, joining and replication. |
| terrain, nav, character, gameplay, save | Terrain, navigation and AI, character controllers, game systems (health, tools, abilities, dialogue) and save files. |

## Design rules

- **One tree of Instances, no entity-component system.** Objects are Instances in one tree, found by name, class or tag. They're stored in an arena with generational IDs, so a removed object can't be reached by mistake.
- **One scripting runtime.** Blocks and graphs become Lua; there's never a second interpreter.
- **The player doesn't contain the editor.** Games ship with only what they need to run.
- **The renderer only draws.** It's given draw lists and knows nothing about Instances or scripts.
- **Desktop first.** Windows and Linux are the main targets, macOS is supported; mobile and web aren't yet.

## Going deeper

The engineering wiki in the source repository describes each system's design, the decisions behind it (with their reasons) and its measured performance:

- [Architecture overview](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Architecture/Architecture-Overview.md)
- [System inventory](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Architecture/System-Inventory.md), with each system's status
- [Architecture decisions](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Architecture/Architecture-Decisions/ADR-Index.md)
