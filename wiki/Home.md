---
title: Axcenity engineering wiki
type: index
status: current
verified: 2026-11-11
verified_commit: 19c7179d
tags: [index]
---

# Axcenity engineering wiki

The engine's persistent engineering memory: what exists, why it exists, how it works, how it performs, and how it should evolve. Source and tests are the final authority; this wiki explains them and links to them. New here? Read the [Wiki guide](Getting-Started/Wiki-Guide.md), then the [Engine overview](Getting-Started/Engine-Overview.md).

> [!tip] For AI agents
> Before changing code: find the system below, read its page and the ADRs it lists, then read the source it names. After changing behavior: update the page, the [Optimization log](Performance/Optimization-Log.md) if you measured something, and [Documentation history](Changelog/Documentation-History.md). The rules are in the repository's `AGENTS.md`.

## Start here

- [Engine overview](Getting-Started/Engine-Overview.md): what Axcenity is, its four hosts, its shape in one page
- [Architecture overview](Architecture/Architecture-Overview.md): layers, boundaries and the big diagram
- [Build and run](Getting-Started/Build-and-Run.md): commands, tools, platform checks
- [System inventory](Architecture/System-Inventory.md): every system with its crate, status and docs

## Architecture

| Page | Covers |
|---|---|
| [Architecture overview](Architecture/Architecture-Overview.md) | Layers, invariants, the main data flows |
| [Repository structure](Architecture/Repository-Structure.md) | What each top-level folder and crate holds |
| [System dependencies](Architecture/System-Dependencies.md) | The crate graph and who may depend on whom |
| [Execution lifecycle](Architecture/Execution-Lifecycle.md) | Startup of each host, the frame, shutdown |
| [Threading model](Architecture/Threading-Model.md) | The main thread and every worker |
| [Memory architecture](Architecture/Memory-Architecture.md) | Arena, IDs, properties, caches, GPU memory |
| [Platform support](Architecture/Platform-Support.md) | Windows, Linux, macOS; backends; deferred targets |
| [Architecture decisions](Architecture/Architecture-Decisions/ADR-Index.md) | Every ADR, accepted and superseded |

## Engine systems

| Simulation | Presentation | Content and data |
|---|---|---|
| [Core and data model](Engine-Systems/Core-and-Data-Model.md) | [Rendering](Engine-Systems/Rendering.md) | [Project and serialization](Engine-Systems/Project-and-Serialization.md) |
| [Frame schedule](Engine-Systems/Frame-Schedule.md) | [Lighting and atmosphere](Engine-Systems/Lighting-and-Atmosphere.md) | [Saves](Engine-Systems/Saves.md) |
| [Scripting](Engine-Systems/Scripting.md) | [Scene bridge](Engine-Systems/Scene-Bridge.md) | [Terrain](Engine-Systems/Terrain.md) |
| [Blocks and graphs](Engine-Systems/Blocks-and-Graphs.md) | [Animation](Engine-Systems/Animation.md) | [Splines and procedural](Engine-Systems/Splines-and-Procedural.md) |
| [Physics](Engine-Systems/Physics.md) | [Audio](Engine-Systems/Audio.md) | [Mesh editing and CSG](Engine-Systems/Mesh-Editing-and-CSG.md) |
| [Characters](Engine-Systems/Characters.md) | [UI](Engine-Systems/UI.md) | [World streaming](Engine-Systems/World-Streaming.md) |
| [Navigation and AI](Engine-Systems/Navigation-and-AI.md) | | [Networking](Engine-Systems/Networking.md) |
| [Gameplay systems](Engine-Systems/Gameplay-Systems.md) | | |

## Editor

- [Editor architecture](Editor/Editor-Architecture.md): `EditorApp`, docking, the viewport, Play, plugins
- [Specialized editors](Editor/Specialized-Editors.md): Model Studio, Animation, UI Designer, Material, Sequencer, graphs, Terrain Studio
- [MCP server](Editor/MCP-Server.md): `axcenity-editor --mcp` for AI clients
- [Toolbox](Editor/Toolbox.md): online asset sources and the central asset directory

## Asset pipeline

- [Pipeline overview](Asset-Pipeline/Pipeline-Overview.md): the lifecycle from import to GPU
- [Importers](Asset-Pipeline/Importers.md) · [Cooking](Asset-Pipeline/Cooking.md) · [Supported formats](Asset-Pipeline/Supported-Formats.md)

## Performance

- [Performance overview](Performance/Performance-Overview.md): where frame time goes today
- [Benchmarking](Performance/Benchmarking.md) · [Profiling](Performance/Profiling.md) · [Hardware notes](Performance/Hardware-Notes.md)
- [Optimization log](Performance/Optimization-Log.md): every measured optimization, problem to result
- [Benchmark history](Performance/Benchmark-History.md): dated numbers, never overwritten
- [Known bottlenecks](Performance/Known-Bottlenecks.md): open issues and their evidence

## Development

- [Coding standards](Development/Coding-Standards.md) · [Implementation patterns](Development/Implementation-Patterns.md)
- [Testing strategy](Development/Testing-Strategy.md) · [Debugging](Development/Debugging.md)
- [Contribution workflow](Development/Contribution-Workflow.md): branches, local verification, docs in the same change · [Release process](Development/Release-Process.md)
- Guides: [Add a script API](Guides/Recipes/Add-a-Script-API.md) · [Add a frame step](Guides/Recipes/Add-a-Frame-Step.md) · [Add a dependency](Guides/Recipes/Add-a-Dependency.md) · [Change a file format](Guides/Recipes/Change-a-File-Format.md) · [Add a render feature](Guides/Recipes/Add-a-Render-Feature.md) · [Optimize safely](Guides/Optimize-Safely.md) · [Troubleshooting](Guides/Troubleshooting.md)

## Reference

[Dependencies](Reference/Dependencies.md) · [File formats](Reference/File-Formats.md) · [Configuration](Reference/Configuration-Reference.md) · [Terminology](Reference/Terminology.md)

## Roadmap and history

- [Current roadmap](Roadmap/Current-Roadmap.md): modernization items I1–L8, plan phases, usability work
- [Known limitations](Roadmap/Known-Limitations.md) · [Technical debt](Roadmap/Technical-Debt.md)
- [Engineering changelog](Changelog/Engineering-Changelog.md) · [Documentation history](Changelog/Documentation-History.md)

## Recently updated

| Date | Page | Change |
|---|---|---|
| 2026-11-14 | ADR-0026, roadmap | Import formats and plugins (P7): USD converted on import, OBJ, mesh import presets, plugin API 3 |
| 2026-11-13 | Rendering, lighting and atmosphere | Sun shafts in rooms: dust in a GraphicsVolume, volumetric fog that stays dark indoors, leaf-shaped ray traced shadows |
| 2026-11-12 | Roadmap | Cohesion pass CP9: final checks done, the pass is complete |
| 2026-11-12 | Project and serialization, roadmap | Cohesion pass CP8: your game's own icon and banner, in the player and every build |
| 2026-11-12 | System inventory, roadmap | Cohesion pass CP7: a Team Projects start page, the host asked before someone joins, narrower listening, firewall checks |

Keep the five newest rows here; the full list is [Documentation history](Changelog/Documentation-History.md).
