---
title: System inventory
type: architecture
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [Cargo.toml, crates/sim/src/steps.rs, crates/sim/src/lib.rs]
docs: [docs/architecture/crates.md, docs/README.md]
tags: [architecture, index, audit]
---

# System inventory

The result of the Phase 1 repository audit (2026-11-10, `main` at `f47d5f0`), brought up to date on 2026-11-11 at `36bcf356` (four new crates: `gameplay`, `mcp`, `release`, `toolbox`). It lists every engine system, where it lives, what state it is in, and where its detailed documentation is. It is the **system directory** the rest of the wiki hangs off.

> [!info] How to read the status column
> **Implemented**: built, tested, on by default. **Partial**: built, but with a known part missing or off by default. **Experimental**: built, opt-in, or dev-only. **Planned**: in a plan or roadmap, not built. **Deferred**: decided not to build until a named milestone ([development policy](../Development/Contribution-Workflow.md#the-development-policy)). **Deprecated**: replaced, kept only for compatibility.

## The codebase at a glance

| Measure | Value |
|---|---|
| Rust | 33 crates under `crates/` (28 at the first audit), about 466k lines including tests at the first audit |
| WGSL | 24 shader files in `crates/render`, about 5.1k lines |
| Luau | about 108 `.lua` files (examples, templates, the test runner's `runner.lua`) |
| Tests | about 3,150 `#[test]` functions (editor 956, script 393, render 277, project 162, player 158, scene 157) |
| Examples | 67 example projects under `examples/` (55 `hello-*` scenes are rendered by `every_example_renders`) |
| Templates | 6 genre templates under `templates/` |
| Toolchain | Rust 1.97.0 pinned in `rust-toolchain.toml`, edition 2024, resolver 3 |
| Platforms | Windows and Linux primary, macOS secondary, mobile and web deferred |
| CI | GitHub Actions runs only for release tags; CI only by hand; local tests and `scripts/check-platform.*` are the verification |
| Version | `0.2.0-alpha.1` in `[workspace.package]` (the one version source) |

## Languages and their responsibilities

| Language | Where | Responsibility |
|---|---|---|
| Rust | `crates/*` | The whole engine, editor, player, server and tools. `unsafe_code = "deny"` everywhere except `script/src/debugger.rs` |
| Luau | creator scripts, `crates/testing/src/runner.lua`, examples, templates, editor plugins | Gameplay. One runtime (`mlua` 0.12 with `luau0-src` pinned `=0.22.0`). Replaced Lua 5.4 after v1.5 ([ADR-0004](Architecture-Decisions/ADR-0004-Luau-As-The-Only-Script-Runtime.md)) |
| WGSL | `crates/render/src/**/*.wgsl` | Every GPU shader. Material files can carry a `surface` function in WGSL, checked by `naga` |
| JSON | project files (`axcenity.json`, `world.json`, `*.anim.json`, `*.material.json`, …) | Saved data. Deterministic, diff-friendly ([File formats](../Reference/File-Formats.md)) |
| CSS | `ui/*.css` in projects | Game UI styling, parsed by `crates/ui/src/style.rs` and laid out with `taffy` |
| C / C++ (vendored, built with `cc`) | `luau0-src`, `ufbx`, `meshopt`, `intel_tex_2` (prebuilt ISPC) | Script VM, FBX reading, mesh simplification, BC7/BC5 encoding (editor only) |
| Python, PowerShell, shell | `scripts/` | Manual platform and package checks |

## Hosts (binaries)

Four programs run the same frame schedule (`crates/sim`, roadmap M1). See [Execution lifecycle](Execution-Lifecycle.md).

| Binary | Crate | What it is | Draws | Audio | Network |
|---|---|---|:-:|:-:|:-:|
| `axcenity-editor` | `editor` | eframe/egui editor; Play runs the game in-process on a cloned world | yes | yes | loopback only |
| `axcenity-player` | `player` | The runtime shipped with games; winit window | yes | yes | host or join |
| `axcenity-server` | `server` | Headless dedicated server; links no window, GPU or audio | no | no | host |
| `axcenity-test` | `testing` | Game test runner (`tests/*.spec.lua`) and `--bench` host | no | no | no |
| `axcenity-bench-check` | `testing` | Regression check against `benchmarks/baselines.json` | | | |

## Systems

Every major system, mapped to its crate, its state and its documentation. "Wiki" is the page in this vault; "Topic docs" are the short per-change files in the repo's `docs/` folder, which stay the detailed reference.

| System | Crates (lines) | Status | Wiki | Topic docs |
|---|---|---|---|---|
| Instance tree, arena, properties, events, change log, class and tag index | `core` (13.5k) | Implemented | [Core and data model](../Engine-Systems/Core-and-Data-Model.md) | `docs/architecture/data-model.md` |
| Frame schedule shared by all hosts | `sim` (1.1k) | Implemented (M1) | [Frame schedule](../Engine-Systems/Frame-Schedule.md) | `docs/architecture/frame-loop.md` |
| Renderer (wgpu 25): PBR, cascaded and soft shadows, tiled lights, LOD, culling, GPU skinning and particles, post, TAA | `render` (33.7k + 5.1k WGSL), `render-types` (1.7k) | Implemented | [Rendering](../Engine-Systems/Rendering.md) | `docs/rendering.md` (map) |
| Lighting and atmosphere: sky, time of day, environment maps, reflection and light probes, real-time GI, fog, clouds, weather | `render`, `scene` | Implemented; real-time GI is the optional tier | [Lighting and atmosphere](../Engine-Systems/Lighting-and-Atmosphere.md) | `docs/rendering/sky.md`, `fog.md`, `clouds.md`, `weather.md` |
| World to draw list: retained draw items, mesh cache, picking, particles host state | `scene` (13.5k) | Implemented (M4 retained) | [Scene bridge](../Engine-Systems/Scene-Bridge.md) | `docs/rendering/pipeline.md`, `docs/rendering/performance.md` |
| Asset loading: glTF, FBX, textures, materials, LODs, cooked copies, asset IDs | `assets` (16.0k) | Implemented; references still name paths | [Asset pipeline overview](../Asset-Pipeline/Pipeline-Overview.md) | `docs/project/assets.md`, `docs/rendering/models.md`, `docs/rendering/textures.md` |
| Texture cooking (BC7, BC5 normals) | `cook` (0.6k), editor only | Implemented (L2) | [Cooking](../Asset-Pipeline/Cooking.md) | `docs/rendering/textures.md` |
| Physics (rapier 0.36, `parallel`): assemblies, joints, movers, vehicles, ragdolls, destruction, queries | `physics` (9.7k) | Implemented | [Physics](../Engine-Systems/Physics.md) | `docs/architecture/physics.md`, `fracture.md` |
| Character controllers, seats | `character` (4.1k) | Implemented | [Characters](../Engine-Systems/Characters.md) | `docs/scripting/api-character.md` |
| Navigation mesh, agents, perception, behavior trees | `nav` (5.1k) | Implemented (M13 worker) | [Navigation and AI](../Engine-Systems/Navigation-and-AI.md) | `docs/architecture/navigation.md` |
| Animation: clips, state machines, layers, blend spaces, IK, ragdoll poses, sequences | `anim` (11.7k) | Implemented; CPU posing, GPU skinning in `render` | [Animation](../Engine-Systems/Animation.md) | `docs/rendering/animation.md`, `docs/project/animation-files.md` |
| Audio (kira 0.12): spatial sound, buses, effects, reverb zones | `audio` (2.3k) | Implemented | [Audio](../Engine-Systems/Audio.md) | `docs/architecture/play-mode.md` (Audio), `docs/scripting/api-sound.md` |
| Scripting: Luau sandbox, scheduler, watchdog, engine API, gameplay services | `script` (52.9k) | Implemented; native `vector` not chosen | [Scripting](../Engine-Systems/Scripting.md) | `docs/scripting.md` (map) |
| Gameplay systems: Debris, Abilities and Effects, Tools, Health and Humanoids, dialogues and quests | `gameplay` | Implemented (L7) | [Gameplay systems](../Engine-Systems/Gameplay-Systems.md) | `docs/architecture/game-systems.md` |
| Blocks, node graphs, UI Event Graphs, behavior-tree and dialogue nodes (all compile to Lua) | `blocks-runtime` (6.8k), `blocks` (8.1k, editor) | Implemented | [Blocks and graphs](../Engine-Systems/Blocks-and-Graphs.md) | `docs/scripting/blocks.md`, `graphs.md`, `ui-graphs.md` |
| Game UI: CSS cascade, taffy layout, widgets, focus, localization, accessibility | `ui` (16.2k) | Implemented (M14 cache) | [UI](../Engine-Systems/UI.md) | `docs/ui.md` |
| Terrain: heightfield, brushes, generation, erosion, chunk meshes, foliage | `terrain` (4.4k) | Implemented | [Terrain](../Engine-Systems/Terrain.md) | `docs/terrain.md` |
| Networking (renet 2, postcard wire): hosting, joining, LAN discovery, replication, motion, ownership | `net` (8.8k), `server` (0.6k) | Partial: local and LAN; internet hosting and secure netcode deferred (M11) | [Networking](../Engine-Systems/Networking.md) | `docs/architecture/networking.md` |
| World streaming | `core::streaming`, `scene/src/streaming.rs`, `net/src/streaming.rs` | Partial: off by default (`StreamingEnabled`) | [World streaming](../Engine-Systems/World-Streaming.md) | `docs/architecture/streaming.md` |
| Saves: slots, checkpoints, per-player data | `save` (2.3k) | Implemented | [Saves](../Engine-Systems/Saves.md) | `docs/architecture/saves.md` |
| Project format: load, save, migrate, levels, prefabs, packing, versions, watcher | `project` (15.9k) | Implemented | [Project and serialization](../Engine-Systems/Project-and-Serialization.md) | `docs/project-and-export.md` (map) |
| Splines and procedural content (SplineMesh, SplineScatter) | `procedural` (1.5k), `core::spline` | Implemented | [Splines and procedural](../Engine-Systems/Splines-and-Procedural.md) | `docs/architecture/splines.md` |
| Mesh editing: plane cuts, fracture, CSG, the Model Studio's meshes | `mesh_edit` (22.8k) | Implemented | [Mesh editing and CSG](../Engine-Systems/Mesh-Editing-and-CSG.md) | `docs/architecture/fracture.md`, `solid-modeling.md`, `docs/editor/model-studio.md` |
| Editor shell, viewport, Play, panels, specialized tabs | `editor` (168k), `editor-kit` (4.8k), `editor-toolbox` (4k), `editor-benchmark` (3k), `editor-tools` (9k), `editor-model-studio` (13k), `editor-node-graph` (2.6k), `editor-ui-designer` (13.5k) | Implemented; splitting into sub-crates (G11/L6): shared UI base, Toolbox, Graphics Benchmark, shared tools and types, Model Studio, node-graph editor and UI Designer done | [Editor architecture](../Editor/Editor-Architecture.md) | `docs/architecture.md` (Editor section) |
| Collaborate (called Team Create before; ADR-0021): sync core (operations, conflict rules, host and client, capturing any World edit from the World's edit journal) and network (device keys, invite links, handshake, WebSocket over TLS to a pinned certificate, reconnecting sessions, the installed Tailscale's state through its CLI so invites name the host's Tailscale name), roster and presence, the project's files (content-addressed saves with compare-and-swap, document locks, each member's mirror folder with conflict copies), and live Lua scripts and stylesheets everyone types in at once (`live.rs`, `yrs` documents the host checks and writes back to the files, kept across host restarts), a session chat with others' cursors in script tabs, and the host's checkpoints with a restore for everyone (`history.rs`), and per-member budgets for each message kind, handshake limits and an audit log for the Owner (`limits.rs`, `audit.rs`; security review in `collaborate.md` §6.1), who gets in (`handshake::Trust`: invite holders at once, or the host asked about new members or everyone), hosts listening only on loopback and their Tailscale or LAN address, and read-only firewall checks with a Windows Allow on click (`firewall.rs`; cohesion pass CP7, `collaborate.md` §5.9); the editor side is `editor/src/app/team.rs`, the Collaborate panel `editor/src/app/team_panel.rs` (its chat `team_chat.rs`, audit log `team_audit.rs`) and the setup wizard and Start Session dialog `editor/src/app/collab_setup.rs`, the Someone wants to join window and trust settings `editor/src/app/collab_trust.rs`, the start screen's Team Projects page `editor/src/start/team.rs` | `collab` (9.4k, plus 4.2k of tests), `collab-server` (the self-hosted `axcenity-collab` server) | In progress (TC1–TC7 built; Collaborate C1–C11 built, C12 the account integration as a document, `docs/research/collaborate-accounts.md`) | [ADR-0020](Architecture-Decisions/ADR-0020-Team-Create-Session-Authority-And-Operations.md), [ADR-0021](Architecture-Decisions/ADR-0021-Collaborate-Tailscale-Live-Scripts-And-Identity-Seam.md) | `docs/research/team-create.md`, `docs/research/collaborate.md` |
| Collaborate 2.0 persistent projects (ADR-0022): the project ID in `collaborate.json`, the members list every would-be host keeps (`ToClient::Members`), each computer's project records, rejoining and hosting for the members without an invite (`crates/collab/src/project.rs`, `crates/editor/src/app/collab_project.rs`) | `collab`, `editor` | In progress (P3 built) | [ADR-0022](Architecture-Decisions/ADR-0022-Persistent-Collaborate-Projects-Git-Sync-And-Host-Handoff.md) | `docs/research/collaborate-persistent.md` |
| Collaborate 2.0 host handoff (ADR-0022): successor list from members' host cards (`crates/collab/src/election.rs`), the host handing the session on and a new host recovering a lost one's transactions from members' tails (`crates/collab/src/host/handoff.rs`), the editor following a handoff or a lost host and taking over (`crates/editor/src/app/collab_handoff.rs`) | `collab`, `editor` | Built (P4; scenarios A and B end to end in P7) | [ADR-0022](Architecture-Decisions/ADR-0022-Persistent-Collaborate-Projects-Git-Sync-And-Host-Handoff.md) | `docs/research/collaborate-persistent.md` §8.4 |
| Collaborate 2.0 Git side (ADR-0022): a project's optional remote repository: the installed `git` (`crates/repo/src/git.rs`), GitHub's device sign-in and REST calls (`github.rs`), the token in Git's own credential store (`credentials.rs`), linking a folder without overwriting files (`repository.rs`), `RepositoryManager` (`manager.rs`), automatic checkpoints and sync (`checkpoint.rs`, `sync.rs`, driven by `crates/editor/src/app/collab_repo.rs`), conflicts kept both ways and resolved per file (`conflicts.rs`, `crates/editor/src/app/collab_conflicts.rs`); checkpoints when a member leaves, hosting passes on or the session ends, and a successor's copy taking up the history (`repo_link_mirror`, `repository::bring_back_missing`, P7); the panel's Project section, sync chip and Repository, History and Resolve windows (`crates/editor/src/app/collab_repo_ui.rs`); the link and the project's persistent ID in `collaborate.json` (`crates/project/src/collaborate_file.rs`) | `repo` | Built (P1–P7; hand checks with real GitHub pending) | [ADR-0022](Architecture-Decisions/ADR-0022-Persistent-Collaborate-Projects-Git-Sync-And-Host-Handoff.md) | `docs/research/collaborate-persistent.md` |
| Teamwork (T1–T4 of `docs/research/teamwork.md`): the Team Board, the project's tasks, bugs and goals (progress from their tasks and bugs; a My work view), tasks and bugs (severity, steps, found in / fixed in; reported from an Output error or an Explorer row) with assignees, status, priority, due dates, checklists, links, comments and activity, kept as one JSON file per item in `team/board/` (`crates/teamwork`) so Collaborate's mirror and the project's repository carry it; two people's copies merged on load; the Team Board tab and item window (`crates/editor/src/team_board.rs`, `crates/editor/src/app/team_board.rs`) ; each person's history of changes in `team/history/<person>/<month>.<computer>.jsonl`, recorded by the host in a session and by each editor outside one, and the People window | `teamwork` | Built (T1–T4) | — | `docs/editor/team-board.md` |
| MCP server for AI clients (`axcenity-editor --mcp`) | `mcp`, `editor` | Implemented | [MCP server](../Editor/MCP-Server.md) | `docs/editor/mcp.md` |
| Toolbox: online asset sources, central asset directory | `toolbox`, `editor` | Implemented (G1) | [Toolbox](../Editor/Toolbox.md) | `docs/research/asset-sources/` |
| Release system: version, channel, history, `axcenity-release`; updater | `release`, `updater` | Implemented | [Release process](../Development/Release-Process.md) | `docs/project/releases.md` |
| Editor Lua plugins | `plugins` (1.5k) | Implemented | [Editor architecture](../Editor/Editor-Architecture.md#plugins) | `docs/editor/plugins.md` |
| Game test runner, S10k benchmark, regression check, XB10k cross-engine benchmark | `testing` (2.6k) | Implemented | [Testing strategy](../Development/Testing-Strategy.md), [Benchmarking](../Performance/Benchmarking.md) | `docs/project/game-tests.md`, `docs/benchmarks.md` |
| Build: workspace, hakari, profiles, platform checks, packaging | root `Cargo.toml`, `crates/workspace-hack`, `scripts/`, `packaging/` | Implemented; CI deferred | [Build and run](../Getting-Started/Build-and-Run.md) | `docs/development.md`, `docs/project/releases.md` |

### Systems the brief asked about that Axcenity does not have as separate systems

| Asked for | What exists instead |
|---|---|
| An ECS or entity system | No ECS, by decision ([ADR-0002](Architecture-Decisions/ADR-0002-Instance-Tree-In-A-Generational-Arena.md)). Entities are Instances in one tree; "find all X" uses the class and tag index (M2) |
| Memory manager or custom allocators | None. Ownership is plain Rust: one owned `World`, `Arc` handles for shared asset data, caches that free by use. See [Memory architecture](Memory-Architecture.md) |
| Render graph | Not built, by decision ([ADR-0009](Architecture-Decisions/ADR-0009-No-Render-Graph-Yet.md)). `ScenePipeline::draw` orders passes by hand |
| Shader cache on disk | None. Built-in pipelines compile at start; surface-shader pipelines are cached in memory by code hash (`surface_shaders.rs`) |
| Platform abstraction layer | None beyond winit, wgpu, kira and `assets` as the only file reader. See [Platform support](Platform-Support.md) |

## Workflows traced through the source

| Workflow | Wiki page |
|---|---|
| Asset lifecycle: import → sidecar ID → cook → load (worker) → scene → GPU upload → budget → free | [Asset pipeline overview](../Asset-Pipeline/Pipeline-Overview.md) |
| Startup of each host, then the frame | [Execution lifecycle](Execution-Lifecycle.md) |
| A script from file to coroutine, events and the watchdog | [Scripting](../Engine-Systems/Scripting.md#execution-flow) |
| A frame drawn: world → draw list → plan → passes | [Rendering](../Engine-Systems/Rendering.md#execution-flow) |
| A physics step: sync → rapier → touches → write-back | [Physics](../Engine-Systems/Physics.md#execution-flow) |
| Animation: Animator → state machine → pose → GPU skin | [Animation](../Engine-Systems/Animation.md#execution-flow) |
| Multiplayer: handshake → snapshot → replication → motion | [Networking](../Engine-Systems/Networking.md#architecture) |
| Editor Play: autosave → clone → session frames → Stop | [Editor architecture](../Editor/Editor-Architecture.md#play-mode) |

## Existing knowledge this wiki builds on

The repository already documents a lot. The wiki links to it and does not copy it ([ADR-0012](Architecture-Decisions/ADR-0012-Wiki-Complements-Topic-Docs.md)).

| Source | What it holds | How the wiki uses it |
|---|---|---|
| `docs/` topic files (about 100) | One file per kind of change, kept short for agents | System pages link to the topic file for detail |
| Crate `CLAUDE.md` files (28) | What the crate is and its local rules | System pages summarize the rules under Development guidance |
| `docs/research/engine-modernization-report.md` and its working notes | The 2026-11-10 audit, roadmap I1–L8, peer-engine comparison | [Current roadmap](../Roadmap/Current-Roadmap.md), [Known bottlenecks](../Performance/Known-Bottlenecks.md) |
| `docs/development-policy.md` | Paul's standing rules of 2026-11-10 | [ADR-0011](Architecture-Decisions/ADR-0011-Desktop-First-Local-Verification.md), [Contribution workflow](../Development/Contribution-Workflow.md) |
| `docs/benchmarks.md`, `docs/rendering/performance.md`, `benchmarks/` | Methods, baselines, before/after tables | [Optimization log](../Performance/Optimization-Log.md), [Benchmark history](../Performance/Benchmark-History.md) |
| `docs/research/usability/` | Usability audit (U*), design-system audit (DS1–DS27, V1–V26) | [Current roadmap](../Roadmap/Current-Roadmap.md#editor-usability-and-design-system) |
| `plans/` and `prd/` | Phased plans with acceptance criteria and "Architectural decisions" sections | [Architecture decisions](Architecture-Decisions/ADR-Index.md) |

## Documentation priorities

What the audit found most valuable to write first, in order:

1. **Architecture overview, crate dependency graph and execution lifecycle.** The rules exist but are spread over `CLAUDE.md`, `crates.md` and plan headers; nobody has drawn them.
2. **Architecture decision records.** Decisions are scattered across five plans, the policy and topic files, and one (Lua 5.4) was silently superseded. Agents re-open settled questions (ECS, render graph, native `vector`) without them.
3. **Optimization log and benchmark history.** Seventeen measured optimizations landed on 2026-11-09 and 2026-11-10 (PRs #4–#64). The numbers are in long tables; the problem, cause and trade-off of each are not in one place.
4. **Asset pipeline lifecycle.** L2 made it real in eight PRs; the path from import to GPU crosses five crates.
5. **System pages** for every row above, each pointing to its topic docs and source.
6. **Known limitations, technical debt and open performance issues**, separated from what is built.
7. **Agent instructions** that send agents here first and keep pages in step with code.
