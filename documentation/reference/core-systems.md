---
description: Each engine system in one table, with what it does, how finished it is and where to read more.
icon: gear
---

# Core systems

Every major system, what it covers and how finished it is. "Partly built" means it works but a known part is missing or it's off until you turn it on.

| System | What it covers | Status | Read more |
|---|---|---|---|
| Instances | The object tree, properties, attributes, tags, events | Stable | [Instances, properties and events](instances.md) |
| Scripting | Luau, the sandbox, the scheduler, the engine API | Stable | [Scripting reference](scripting.md) |
| Blocks and graphs | Block scripts, the Graph view, UI Event Graphs, behaviour trees, dialogues: all become Lua | Stable | [Blocks: visual programming](../learn/blocks.md) |
| Rendering | PBR materials, shadows, many lights, levels of detail, culling, post effects, anti-aliasing | Stable | [Rendering and graphics](rendering.md) |
| Lighting and sky | Time of day, sky, atmosphere, clouds, weather, fog, reflection and light probes | Stable; real-time global illumination is an opt-in quality tier | [Lighting and rendering](../learn/lighting-and-rendering.md) |
| Physics | Rigid bodies, collisions, joints, movers, vehicles, ragdolls, destruction, spatial queries | Stable | [Physics](physics.md) |
| Characters | First- and third-person controllers, Humanoids, seats, tools, health, abilities | Stable | [Common classes](classes.md) |
| Navigation and AI | Navigation meshes, agents with sight and hearing, behaviour trees | Stable | [Common classes](classes.md) |
| Animation | Clips, state machines, blend trees, layers, IK, property animation, cutscenes | Stable; animation in glTF files isn't imported | [Animation](animation.md) |
| Audio | Sounds, spatial sound, rolloff, occlusion, buses, effects, reverb zones | Stable; stereo panning only | [Audio](audio.md) |
| Game UI | Screen and billboard UI, CSS styling, widgets, localisation, accessibility | Stable | [UI](ui.md) |
| Terrain | Heightfield terrain, sculpting and painting brushes, generation, erosion, foliage | Stable | [Terrain tools](../learn/terrain.md) |
| Splines | Paths for roads, rails and patrols, meshes and scatter along them | Stable | [Common classes](classes.md) |
| Modeling | Unions and negates (CSG), the Model Studio for editing meshes, fracture | Stable | [Building and moving objects](../learn/building.md) |
| Saves and levels | Save slots, checkpoints, per-player data; several levels per game | Stable | [Services and globals](services.md) |
| Assets | Importing, cooking textures for the GPU, asset IDs, the Toolbox | Stable | [Assets and importing](assets.md) |
| Multiplayer | Hosting, joining, LAN discovery, replication, ownership | Partly built: LAN and trusted direct play only | [Multiplayer and networking](networking.md) |
| World streaming | Loading only what's near each player | Partly built: off unless `Workspace.StreamingEnabled` is on | [Multiplayer and networking](networking.md) |
| Build Game | Windows, macOS and Linux builds | Stable; macOS builds aren't notarized | [Publishing your game](../learn/publishing.md) |
| Game tests | Automated tests in Lua, run in the editor or a terminal | Stable | [Debugging and profiling](debugging.md) |
| Editor plugins | Lua plugins that add panels and ribbon buttons to the editor | Experimental | [Engine architecture](architecture.md) |
| AI assistant (MCP) | Lets an AI assistant read and edit a project through the editor (`axcenity-editor --mcp`) | Experimental | [Debugging and profiling](debugging.md) |

## Not built

These are common in other engines but Axcenity doesn't have them yet: export to web, mobile or consoles; internet matchmaking, relays or encrypted connections; HRTF (3D-positioned) audio; UI drawn on a part's surface; importing animation from glTF files; and real-time collaboration in the editor (Team Create is being built, but you can't start or join a session from the editor yet).

The source repository's wiki keeps the full [system inventory](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Architecture/System-Inventory.md) and [known limitations](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Roadmap/Known-Limitations.md).
