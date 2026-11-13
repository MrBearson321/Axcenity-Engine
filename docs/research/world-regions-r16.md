# Large-world authoring (R16): regions and data layers

This file covers roadmap item R16, section 6.5 of [competitive-development-roadmap.md](competitive-development-roadmap.md). It checks each of the section's requirements against main as of 13 Nov 2026, after the Streaming overlay (#280), and sets the stages that remain. R16.1 was the Streaming overlay (#280). Each later stage is its own PR. What streaming does today is described in [docs/architecture/streaming.md](../architecture/streaming.md), and levels in [docs/project/levels.md](../project/levels.md).

## Audit

| Requirement | Today | Verdict |
|---|---|---|
| Better world streaming visualization | The Streaming overlay (#280): unit boxes by reach, always-there parts, both radii, what a player standing there gets, and units too big to stream well | Met |
| Streaming diagnostics | The same overlay while editing; Play's rings; `StreamingState` and `StreamedIn` for scripts | Met |
| Improved terrain streaming workflows | Play builds terrain chunks in range on a worker and drops the rest. Editing builds and draws every chunk | **Open**: R16.3 (stream while editing) |
| Region management | Folders and Models group content, `Atomic` and `Persistent` set how they stream, levels split a game into Workspaces, and `AddLevel` puts one beside another. The editor always draws, picks and simulates the whole Workspace, and a part of the world can't be set aside while you work on another | **Open**: R16.3 |
| Data layers | Tags (CollectionService, the Tags panel) group Instances across the tree. Nothing turns a group off in the editor, and turning one off in a game means a script moving it to ServerStorage | **Open**: R16.2 (editor), R16.4 (game) |
| Improved vegetation authoring | Foliage painted on terrain with density, items from the Assets panel, distance culling; SplineScatters | Met for R16; more belongs to the terrain work |
| Better water and environmental effects | Terrain water, weather, clouds, fog volumes | Owned by the lighting thread (LE0–LE7, #318) |
| Improved world-scale lighting workflows | Light probe volumes and GI tiers (R18), sun shadows, ray tracing | Owned by R18 and the RT thread |
| Acceptance: large-world performance is benchmarked | `bench_50000_parts_streaming` and the streaming world's `--bench` | Met; R16.3 adds an editing benchmark |

## Decisions

- **D1. A data layer is a `DataLayer` object, named as the tag it gathers.** Unreal's Data Layers are sets of actors that cut across the folder tree, and an actor can be in more than one. Tags already work this way: an Instance can carry many of them, CollectionService finds them, the Tags panel counts them, the Properties panel edits them as undo steps, and Collaborate syncs them. A layer is a `DataLayer` (its `Name` is the tag, with `Color` and `ActiveOnStart`) inside a new `DataLayerService`, saved in `world.json` as `data_layer_service` beside the other services, so every level shares them. Being Instances, layers get undo, Collaborate, the Properties panel and scripts for free. Tagging a Model puts all of it in the layer. The service is made after every other one, so no other service's ID moves.
- **D2. Showing and locking a layer is per person.** Whether you see a layer, and whether a click can pick it, is a view setting, as in Unreal. It's kept in `.axcenity/layers.json` (this computer's, as `backup.json` is) and never changes the world or the undo history. A hidden layer isn't drawn or picked in the editor, a locked one is drawn but a click goes through it (as with `Locked`), and Play shows every layer that's active.
- **D3. A region is a Folder or Model at the top of the Workspace, and the editor can set one aside.** Unreal cuts the world into grid cells, but Axcenity's streaming already groups content into units and lets the creator choose the groups. The Regions panel lists the Workspace's top-level Folders and Models with their part counts and size. **Unload in editor** stops drawing, picking and simulating a region while editing. It stays in the world and is saved unchanged, so unloading isn't an edit. Like D2, this is per person (`.axcenity/layers.json`).
- **D4. Stream while editing.** With `StreamingEnabled` on, the Viewport menu can make editing draw only what Play's host screen would draw around the camera: the same `scene::Streaming`, terrain chunks built on its worker, and GPU copies freed out of range. A selected Instance is always drawn. This is how a 4 km map stays fast to edit.
- **D5. In a game, an inactive layer's Instances are parked.** `DataLayerService:SetLayerActive(name, active)` moves the layer's tagged Instances (the topmost of each tagged subtree) into the service, which isn't replicated or simulated. Activating it puts them back under their old parents, in their old order. `IsLayerActive(name)` and `LayerChanged(name, active)` report the state. The server decides, and devices see the parts leave and come back as normal replication. This is the ServerStorage pattern that Roblox-style games already write by hand, made into one call with a setting for the starting state.
- **D6. Left out:** HLOD proxy meshes (the renderer's own LODs and streaming cover distance), one file per actor (Collaborate already merges per change, and levels are separate files), and fixed grid cells.

## Stages

| Stage | What | Tests | PC check |
|---|---|---|---|
| R16.2 Data layers in the editor | D1 and D2: `DataLayerService` and `DataLayer` (`core`, saved by `project`), a Layers panel (new layer from the selection, colour, eye, lock, select members, add and remove the selection, rename and delete as undo steps), the viewport leaving out hidden layers and passing over locked ones, Explorer rows dimmed for hidden layers | Layers save and load (and a world without any is unchanged); hidden members aren't drawn or picked; locked ones are drawn but not picked; layer edits undo; the view settings stay out of the world | A layer of a town's lights hidden and shown on a large map |
| R16.3 Regions and streaming while editing | D3 and D4: the Regions panel, Unload in editor and Load, Stream while editing (Viewport menu, Settings ▸ Viewport), the Streaming overlay showing what's set aside | Unloaded regions aren't drawn or picked and are saved unchanged; the draw list while editing with streaming holds only what's in range plus the selection; an editing benchmark on the streaming world | Editing frame time on the 50,000-part streaming world, streaming on and off |
| R16.4 Data layers in games | D5: `DataLayerService`, each layer's starting state applied when a game starts, blocks ("set layer *name* active", "layer *name* is active", "when layer *name* changes"), the API reference | Parking and restoring keeps parents and order; scripts see `LayerChanged` on both sides; a joined device sees the parts leave and come back; save slots keep each layer's state | A layer switched during Play with a joined player |

Each stage updates the topic files (`docs/architecture/streaming.md`, `docs/project/levels.md`, or a new `docs/editor/layers.md`), the wiki's World Streaming page and the roadmap's 6.5 status.
