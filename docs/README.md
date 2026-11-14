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
| The user-facing system requirements (supported systems, minimum and recommended processor, memory, graphics, network, storage) | [../documentation/reference/system-requirements.md](../documentation/reference/system-requirements.md) |
| Build speed, test commands, or nextest settings | [development.md](development.md) |
| Scope and priorities: CI (off on purpose), target platforms, determinism, the Vector3 caveat, dedicated hosting, and the development policy | [development-policy.md](development-policy.md) |
| Whole-frame benchmarks, the S10k scene, `--bench`, profiler spans and Tracy | [benchmarks.md](benchmarks.md) |
| The competitive development roadmap (Paul's of 2026-11-12): each item's status against Unreal, Godot, s&box and Unity, the feature matrix, and the order of work (G items and R12–R20) | [research/competitive-development-roadmap.md](research/competitive-development-roadmap.md) |
| The visual effects editor (R17): decisions D1–D7 (one particle system, NumberSequence and ColorSequence curves, the effect asset as a prefab, a module stack before a node graph) and stages R17.1–R17.5 | [research/vfx-editor.md](research/vfx-editor.md) |
| Model Studio improvements (R15): the eleven requirements checked against main (seven met), decisions D1–D4 (sculpting by brushes and filters, collision as `_COL` objects with MeshPart `CollisionFidelity`, LODs left to the renderer, Blender launched not embedded) and stages R15.1–R15.4 | [research/model-studio-r15.md](research/model-studio-r15.md) |
| Large-world authoring (R16): the section 6.5 audit, data layers as `DataLayer` objects named as tags, per-person layer and region visibility, regions as top-level Folders and Models, streaming while editing, `DataLayerService`, decisions D1–D6 and stages R16.2–R16.4 | [research/world-regions-r16.md](research/world-regions-r16.md) |
| Global illumination improvements (R18): what probe volumes and real-time GI do today, the options weighed (overlay, invalid probes, sky light kept apart, auto placement, tiers; not L2 SH, SSGI, lightmaps or SDFGI), decisions D1–D5 and stages R18.1–R18.4 beside RT2 | [research/global-illumination.md](research/global-illumination.md) |
| Platform expansion (R20, research only): what Android, iOS, the web, PC VR through OpenXR, consoles and advanced simulation would need from the engine, blockers per target, and the spike order | [research/platform-expansion.md](research/platform-expansion.md) |
| Runtime lighting parity (Paul's Priority 5 of 2026-11-14): what a played game gets of the ray tracing and lighting work against the editor, dynamic GI on GPUs without ray tracing (software-traced probes), and items RL1–RL5 (characters in ray traced effects, games turning ray tracing on, software GI, RT reflections, runtime benchmarks) | [research/runtime-lighting-parity.md](research/runtime-lighting-parity.md) |
| The lighting engine audit (Paul's request of 2026-11-13): is the ray tracing real, what's missing, incomplete or unnecessary against Unreal 5 and NVIDIA RTX, and items LE1–LE7 (lamp shadows, fog volumes, traced shafts, terrain water, traced water reflections, path tracing, cut-outs in AO and GI) | [research/lighting-engine-audit.md](research/lighting-engine-audit.md) |
| Optional hardware ray tracing: what wgpu 30's ray queries offer per backend, RX 9070 XT measurements (`benchmarks/ray-tracing-probe`), which effects pay off, denoising, acceleration structures, fallback, other engines, and phases RT0–RT5 | [research/hardware-ray-tracing.md](research/hardware-ray-tracing.md) |
| How Axcenity compares with Unity, Unreal, Roblox Studio, Godot and s&box, the XB10k cross-engine results, and recommendations C1–C10 with their owners | [research/engine-comparison.md](research/engine-comparison.md) |
| The editor window, ribbon, or theme | [editor/shell.md](editor/shell.md) |
| Selection or Properties | [editor/selection.md](editor/selection.md) |
| Move, scale, rotate, or undo | [editor/transforms.md](editor/transforms.md) |
| The Explorer tree | [editor/tree.md](editor/tree.md) |
| Data layers: `DataLayerService`, `DataLayer`, the Layers panel, hidden and locked layers (`.axcenity/layers.json`) | [editor/layers.md](editor/layers.md) |
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
| AutoRecover (Settings > Saving, `.axcenity/autorecover/`, the Recover unsaved work? prompt) | [project/autorecover.md](project/autorecover.md) |
| New Project's genre templates (`templates/`) and your own (`.axtemplate`, Save as Template…) | [project/templates.md](project/templates.md) |
| Game tests (`tests/*.spec.lua`), the Tests panel, `axcenity-test` | [project/game-tests.md](project/game-tests.md) |
| Editor plugins (`plugins/`, the plugin API, the Plugins panel) | [editor/plugins.md](editor/plugins.md) |
| Team Create (real-time collaboration): the proposed architecture, sync model, identity and phases TC0–TC8 | [research/team-create.md](research/team-create.md) |
| Collaborate (Team Create's new name, brief of 2026-11-12): audit of what exists, Tailscale setup, live shared scripts, identity seam, phases C1–C12 | [research/collaborate.md](research/collaborate.md) |
| Collaborate accounts (C12, document only): how a future Django account manager would plug in (tokens bound to device keys, linking members, ownership, the website's API, threat model) | [research/collaborate-accounts.md](research/collaborate-accounts.md) |
| Axcenity Online (R19, document only): optional accounts, publishing with signed game manifests and delta updates, self-hosting then managed hosting, opt-in analytics, the marketplace as a Toolbox provider, the dashboard, the one `online` client crate, threats and order O1–O9 | [research/online-services.md](research/online-services.md) |
| Collaborate 2.0 (brief of 2026-11-12): persistent projects without a fixed host, Git/GitHub sync and checkpoints, automatic host election and handoff, phases P0–P7 | [research/collaborate-persistent.md](research/collaborate-persistent.md) |
| Teamwork (Paul's request of 2026-11-12): the shared Team Board of tasks and assignments, bug tracking, goals, and each person's history, phases T1–T4 | [research/teamwork.md](research/teamwork.md) |
| The Team Board tab: `team/board/` files, sharing, the panel and item window | [editor/team-board.md](editor/team-board.md) |
| The cohesion pass (brief of 2026-11-12): audit of the editor's design system, documents and saving, scripting IRs, previews, physics, Collaborate security and game branding, phases CP1–CP9 | [research/cohesion-pass.md](research/cohesion-pass.md) |
| The MCP server for AI clients (`axcenity-editor --mcp`, its tools, connecting Claude Code, Cursor or Codex) | [editor/mcp.md](editor/mcp.md) |
| Setting up an AI assistant with a project (user guide: `--mcp --check`, client configs, profiles, local models, troubleshooting) | [mcp/README.md](mcp/README.md) |
| MCP measurements: definition tokens, result sizes, workflows, latency, S10k, small local models, the confusing-tool review | [mcp/benchmarks.md](mcp/benchmarks.md) |
| In-game UI, the string table and languages | [ui.md](ui.md) |
| Terrain algorithms | [terrain.md](terrain.md) |

Product scope is one story in `prd/`. The roadmap is one phase in `plans/`. Don't open either file in full.
