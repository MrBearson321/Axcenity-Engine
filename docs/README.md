# Engine docs

Open **one** file for the task. The pages in this folder that share a name with a topic (`architecture.md`, `scripting.md`, `rendering.md`, `project-and-export.md`) are maps. The detail is in the file they name.

Source and tests win when a doc is wrong. Update the topic file you used.

| If you are changing… | Open |
|---|---|
| A crate boundary or a new dependency | [architecture/crates.md](architecture/crates.md) |
| Instances and IDs | [architecture/data-model.md](architecture/data-model.md) |
| The frame loop | [architecture/frame-loop.md](architecture/frame-loop.md) |
| A game system outside `script` (`GameSystem`, the `gameplay` crate, Debris) | [architecture/game-systems.md](architecture/game-systems.md) |
| Physics | [architecture/physics.md](architecture/physics.md) |
| Fractures, the `mesh_edit` crate, or plane cutting | [architecture/fracture.md](architecture/fracture.md) |
| Union, Negate, Intersect, Separate, `UnionOperation`, `CollisionFidelity` | [architecture/solid-modeling.md](architecture/solid-modeling.md) |
| The navigation mesh, agents, or behavior trees | [architecture/navigation.md](architecture/navigation.md) |
| Splines, SplinePoints, SplineFollowers, riding moving parts, patrols, the Spline camera cut, SplineMesh and SplineScatter (the `procedural` crate), Bake, Flatten Terrain Along | [architecture/splines.md](architecture/splines.md) |
| Play and Stop | [architecture/play-mode.md](architecture/play-mode.md) |
| The F9 Developer Console, `ConsoleService`, the player's log files and crash message | [developer-console.md](developer-console.md) |
| Save slots, checkpoints, `SaveService` | [architecture/saves.md](architecture/saves.md) |
| Multiplayer: the `net` crate, `axcenity-server`, `NetworkService`, hosting, joining, LAN discovery, lag compensation, replays (`ReplayService`) | [architecture/networking.md](architecture/networking.md) |
| World streaming: `StreamingEnabled`, `ModelStreamingMode`, `RequestStreamAroundAsync`, `StreamingState`, `Instance.StreamedIn`, unloading from the GPU, far physics sleeping | [architecture/streaming.md](architecture/streaming.md) |
| Levels, `LevelService`, the Levels panel | [project/levels.md](project/levels.md) |
| Cutscene sequences, the Sequence Instance, the Sequencer tab | [project/sequences.md](project/sequences.md) |
| Platforms or file access | [architecture/portability.md](architecture/portability.md) |
| Build speed, test commands, or nextest settings | [development.md](development.md) |
| Scope and priorities: CI (off on purpose), target platforms, determinism, the Vector3 caveat, dedicated hosting, and the development policy | [development-policy.md](development-policy.md) |
| Whole-frame benchmarks, the S10k scene, `--bench`, profiler spans and Tracy | [benchmarks.md](benchmarks.md) |
| How Axcenity compares with Unity, Unreal, Roblox Studio, Godot and s&box, the XB10k cross-engine results, and recommendations C1–C10 with their owners | [research/engine-comparison.md](research/engine-comparison.md) |
| The editor window, ribbon, or theme | [editor/shell.md](editor/shell.md) |
| Selection or Properties | [editor/selection.md](editor/selection.md) |
| Move, scale, rotate, or undo | [editor/transforms.md](editor/transforms.md) |
| The Explorer tree | [editor/tree.md](editor/tree.md) |
| Script tabs, file watching, hot reload of scripts with their state kept, or rebuilding the engine | [editor/scripts.md](editor/scripts.md) |
| Model import or the Animation panel | [editor/import.md](editor/import.md) |
| The Model Studio tab, editable meshes (`EditMesh`, `ModelDoc`), modeling operations (`MeshOp`), modifiers and the `.meshedit.json` sidecar, Generate LODs, saving `.glb` | [editor/model-studio.md](editor/model-studio.md) |
| Paint, materials, lights, or the angular tool | [editor/building.md](editor/building.md) |
| The Assets panel | [editor/assets.md](editor/assets.md) |
| Learn, the Documentation page, the API Reference page, `documentation/` (user docs and GitBook) or generating `documentation/api/` | [editor/learn-and-docs.md](editor/learn-and-docs.md) |
| Scripts, LocalScripts, or RemoteEvents in the editor | [editor/sides.md](editor/sides.md) |
| Lua, the sandbox, blocks, or graphs | [scripting.md](scripting.md), then one file |
| Drawing, materials, lights, or animation playback | [rendering.md](rendering.md), then one file |
| Project files, export, licenses, or GitHub releases | [project-and-export.md](project-and-export.md), then one file |
| Engine version, `--version`, release entries in `releases/`, or `axcenity-release` | [project/releases.md](project/releases.md); the design of the updater and Version Explorer: [research/release-system/README.md](research/release-system/README.md) |
| Data tables, the Data Table tab, or `DataService` | [project/data-tables.md](project/data-tables.md) |
| Dialogues, quests, `DialogueService`, `QuestService`, the Dialogue tab | [scripting/api-dialogue.md](scripting/api-dialogue.md) |
| New Project's genre templates (`templates/`) | [project/templates.md](project/templates.md) |
| Game tests (`tests/*.spec.lua`), the Tests panel, `axcenity-test` | [project/game-tests.md](project/game-tests.md) |
| Editor plugins (`plugins/`, the plugin API, the Plugins panel) | [editor/plugins.md](editor/plugins.md) |
| Team Create (real-time collaboration): the proposed architecture, sync model, identity and phases TC0–TC8 | [research/team-create.md](research/team-create.md) |
| Collaborate (Team Create's new name, brief of 2026-11-12): audit of what exists, Tailscale setup, live shared scripts, identity seam, phases C1–C12 | [research/collaborate.md](research/collaborate.md) |
| Collaborate accounts (C12, document only): how a future Django account manager would plug in (tokens bound to device keys, linking members, ownership, the website's API, threat model) | [research/collaborate-accounts.md](research/collaborate-accounts.md) |
| Collaborate 2.0 (brief of 2026-11-12): persistent projects without a fixed host, Git/GitHub sync and checkpoints, automatic host election and handoff, phases P0–P7 | [research/collaborate-persistent.md](research/collaborate-persistent.md) |
| The cohesion pass (brief of 2026-11-12): audit of the editor's design system, documents and saving, scripting IRs, previews, physics, Collaborate security and game branding, phases CP1–CP9 | [research/cohesion-pass.md](research/cohesion-pass.md) |
| The MCP server for AI clients (`axcenity-editor --mcp`, its tools, connecting Claude Code, Cursor or Codex) | [editor/mcp.md](editor/mcp.md) |
| Setting up an AI assistant with a project (user guide: `--mcp --check`, client configs, profiles, local models, troubleshooting) | [mcp/README.md](mcp/README.md) |
| MCP measurements: definition tokens, result sizes, workflows, latency, S10k, small local models, the confusing-tool review | [mcp/benchmarks.md](mcp/benchmarks.md) |
| In-game UI, the string table and languages | [ui.md](ui.md) |
| Terrain algorithms | [terrain.md](terrain.md) |

Product scope is one story in `prd/`. The roadmap is one phase in `plans/`. Don't open either file in full.
