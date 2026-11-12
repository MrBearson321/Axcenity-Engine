---
title: Current roadmap
type: roadmap
status: current
verified: 2026-11-11
verified_commit: 19c7179d
docs: [docs/research/engine-modernization-report.md, docs/research/engine-gap-analysis.md, docs/development-policy.md, plans/axcenity-engine-unified.md, plans/axcenity-engine-graphics.md, docs/research/usability/design-system.md, docs/research/usability/usability-audit.md, docs/research/release-system/README.md]
tags: [roadmap]
---

# Current roadmap

What's done, open, deferred and planned, as of `36bcf356` (2026-11-11). Sources of truth: the modernization report's section 6 (I/M/L items), the engine gap analysis's section 4 (G items, PR #89), `plans/axcenity-engine-unified.md` (Phases 1–93), `plans/axcenity-engine-graphics.md` (Phases 1–38), and the usability, design-system and release-system research. **Planned** means not built; never describe it as existing.

## Engine modernization (report of 2026-11-10)

```mermaid
flowchart LR
  I1[I1 measure ✓] --> T1["I2–I11 ✓"]
  I1 --> M8[M8 ✓] & M2[M2 ✓]
  M2 --> M1[M1 ✓] & M3[M3 ✓] & M4[M4 ✓]
  M1 & M4 --> M5[M5 ✓] & M6[M6 ✓] & M7[M7 ✓]
  M5 --> L2[L2 ✓]
  M9[M9 partly ✓] --> L2
  M6 --> L3[L3 part 1 ✓]
  M1 & M4 --> L4[L4 measured, pools ✓]
  M2 --> L1[L1 measured: not now]
  M1 --> L7[L7 ✓]
  M12[M12 ✓] --> L5[L5 open]
  M12 --> L8[L8 deferred]
  M12 --> L6[L6 / G11 in progress]
```

| Item | What | Status | PR / evidence |
|---|---|---|---|
| I1–I11 | Measure first; Instance table keys; pins; cheap `with_world`; navmesh; shared meshes; `descendants_iter`; watcher; preload | **Done** | #4–#12 |
| M1 | Shared frame schedule (`sim`) | **Done** | #15, #16, #20, #24 |
| M2 | Class and tag index | **Done** | #17 |
| M3 | Physics sync overhaul + rapier `parallel` | **Done**; physics step part 1 and own pool followed | #25, #84, #86, #93 |
| M4 | Retained draw items | **Done**; next: spatial `ItemChunk`s | #23 |
| M5 | Assets off the lock | **Done** | #18, #19 |
| M6 | Script hot paths | **Done** | #22, #26, #27, #29 |
| M7 | postcard wire, motion revisions | **Done** | #34 |
| M8 | Server without GPU or audio | **Done** | #13 |
| M9 | `.axgame` zstd, times, threaded unpack | **Partly done** (read-in-place and cooked-only are G12) | #61 |
| M10 | CI half | **Deferred** (Actions runs for release tags only, CI by hand; [ADR-0018](../Architecture/Architecture-Decisions/ADR-0018-Actions-For-Release-Tags-Only.md)) | policy |
| M11 | renet secure mode | **Deferred** with internet hosting | policy |
| M12 | wgpu 30, egui 0.36, egui_dock 0.21, glyphon 0.12 | **Done** | #104 |
| M13 | Navmesh worker, shared mesh | **Done** | #32 |
| M14 | UI layout cache | **Done** | #37 |
| L1 | Per-class property slots | **Measured: not now** (name lookups ~9% of S10k; a Part ~3.3 KB; `World::clone` of 10k Parts 11.5 ms). Revisit above ~20% of a frame | #59, #87 |
| L2 | Import pipeline | **Done**, including embedded images (#65) and **references by asset ID** (#78) | #45–#57, #65, #78 |
| L3 | Faster Vector3, opt-in native code | **Part 1 done**; native `vector` **on hold** ([ADR-0015](../Architecture/Architecture-Decisions/ADR-0015-Native-Vector-On-Hold.md)) | #58 |
| L4 | Parallel read / serial write | Query bounds instead of threads (#59); 16-core measurement gave physics and rendering **own pools of ≤12 threads** (#86, #88). Stage-level parallelism still open | #59, #86, #88 |
| L5 | GPU-driven path | **Open**, after M12; the gap analysis puts it in "future, with triggers" | — |
| L6 | Split `editor` (now G11) | **In progress**: `editor-kit`, `editor-toolbox`, `editor-benchmark`, `editor-tools`, `editor-model-studio`, `editor-node-graph`, `editor-ui-designer` | G11 PRs |
| L7 | `gameplay` crate behind `GameSystem` | **Done** (Debris, Abilities, Tools, Health and Humanoids, dialogues and quests); Players stays in `script` ([ADR-0019](../Architecture/Architecture-Decisions/ADR-0019-Gameplay-Systems-Through-GameSystem.md)) | #68, #72, #77, #82, #85 |
| L8 | Render graph | **Deferred** ([ADR-0009](../Architecture/Architecture-Decisions/ADR-0009-No-Render-Graph-Yet.md)) | — |

## Next-stage roadmap (gap analysis of 2026-11, PR #89)

Built after comparing Axcenity with Unity, Unreal, Roblox Studio, Godot and s&box (`docs/research/engine-comparison.md`) and timing XB10k across engines.

| Item | What | Status |
|---|---|---|
| G1 | Toolbox: one provider layer for asset discovery | **Done** ([Toolbox](../Editor/Toolbox.md); #90, #92, #96, #107) |
| G2 | Scripts inside objects (`SourceFile` references, `objects/` folder) | **Done** (#91 design, #111, #114, #115, #117 on players' devices, in levels and characters, #120) |
| G3 | Physics render interpolation | **Done** (#110) |
| G4 | Script completion and diagnostics in the editor | Stage 1 (completion and signature hints) and stage 2 (luau-lsp errors and lints in code tabs) in review |
| G5 | UI prefabs (ScreenGuis and elements with their scripts) | **Done** (#122) |
| G6 | Debugger stepping and watches | Planned |
| G7 | Memory breakdown (Profiler Memory tab and F9 Stats); history, peaks and budgets | **Done** (#125, #126; history, peaks and budgets: competitive roadmap PR) |
| G8 | `RollOffMode` curves and audio occlusion | **Done** (#128) |
| G9 | Upscaling (FSR 1-class) | Stage 1 done (#155; EASU + RCAS below 100% render scale, about 0.13 to 0.25 ms on an RX 9070 XT); stage 2 (upscale inside TAA) not started |
| G10 | Plugin API additions | Planned, as needed |
| G11 | Split the editor (L6) | In progress: `editor-kit` (#162), the Toolbox tab in `editor-toolbox` (#164), Graphics Benchmark in `editor-benchmark` (#166), the viewport tools in `editor-tools` and the Model Studio in `editor-model-studio` (#189; editor rebuild after an edit 35.5 → 32.5 s in the cloud), the node-graph editor in `editor-node-graph` and the UI Designer in `editor-ui-designer` (editor rebuild after an edit 30.7 s) |
| G12 | Cooked-only games, read the pack in place | Planned |
| G13 | 16–32 client LAN test | Planned, when the PC is free |
| G14–G16 | Run without a player; physics debug views; humanoid retargeting | Planned |
| TC1–TC8 | Team Create (live collaboration; brief of 2026-11-11 §8): design approved ([ADR-0020](../Architecture/Architecture-Decisions/ADR-0020-Team-Create-Session-Authority-And-Operations.md), #134); TC1 sync core in `collab` (#140); TC2 editor sync (edit journal, capture, two-editor tests, #150); TC3 network and identity (device keys, invites, TLS WebSocket, `--team-host`/`--team-join`, #154); TC4 interface (roster and presence #156, panel, title bar chip, others' selections #160); TC5 files (content-addressed saves with compare-and-swap, document locks, members' mirror folders, conflict copies; sync core #168, editor #169, every document tab and start-screen Join #170); TC6 reliability (host journal and crash recovery, size and rate limits #175; host epochs, members' recovery files, Recovered Changes, read-only when the host leaves, camera follow, Settings > Team Create, #180); TC7 the self-hosted `axcenity-collab` server (persistence, crash recovery, admin command line); tested on two machines over a LAN with a 5-minute cut, both ending identical (2026-11-12, results in `docs/research/team-create.md` §12; PR #195) | Done except TC8, which C12 replaces |
| C1–C12 | Collaborate (Team Create renamed; Paul's brief of 2026-11-12): audit and plan in `docs/research/collaborate.md`, [ADR-0021](../Architecture/Architecture-Decisions/ADR-0021-Collaborate-Tailscale-Live-Scripts-And-Identity-Seam.md); C1–C6 merged (audit, protocol and rename, Tailscale, setup wizard, recovery, scene presence; two-machine tests passed), C7 live scripts (`yrs`) merged; C8 chat and cursors merged; C9 host checkpoints and restores merged; C10 permissions and hardening built (identity seam, budgets per message kind, audit log, security review); C11 performance built (big messages packed with zstd, S10k measured: idle under 0.1 ms, and S10k's load 5.7 → 0.19 s); C12 Django account integration written as a document only (`docs/research/collaborate-accounts.md`) | In progress |
| P0–P7 | Collaborate 2.0 (Paul's brief of 2026-11-12, persistent collaboration and automatic host handoff): design in `docs/research/collaborate-persistent.md`, [ADR-0022](../Architecture/Architecture-Decisions/ADR-0022-Persistent-Collaborate-Projects-Git-Sync-And-Host-Handoff.md); P0 design done (#208); P1 repository integration (#209), P2 automatic sync and checkpoints (#212) P3 persistent projects (#213), P4 host election and handoff (#214), P5 conflicts and recovery (#217), P6 interface (#218) and P7 scenarios A–E end to end and the guide built; PRs await merge in order and Paul's hand checks | In review |
| Priority 4 | GPU-driven drawing, occlusion culling, dynamic GI, ray tracing, AI assistant in the editor, Parallel Luau, … | Future, each with a trigger in the gap analysis |

## Feature plan (unified plan, Phases 1–93)

| Range | Status |
|---|---|
| Phases 1–80 | **Built**; six hand checks need the developer's PC, Mac or two computers, and Phase 80's 4 × 4 km benchmark scene is now built (`axcenity-test --make-streaming`, 2026-11-12; the server's frame on it went 21.9 → 4.2 ms in the container) with its desktop numbers measured on the PC (the plan's "Audit of 2026-11-12") |
| Phases 60–61: sculpting, retopology and baking | **Removed** by Phase 90 (narrower Model Studio, done 2026-11-12, about 5,600 lines out): creators sculpt and bake in Blender and import the `.glb`. Rigging keeps the pieces it used (`mesh_edit` `bvh.rs`, `reference.rs`, `remesh.rs`) |
| Phase 81: lag compensation and replays | **Built** (2026-11-12): server raycasts rewound to where the shooter saw moving parts (`RaycastParams.CompensateFor`, `Tool.LagCompensated`, network protocol 12) and `ReplayService` recordings, `.replay` files, `axcenity-player --replay` and `PlayBuffer` kill cams; recording and watching a replay by hand is checked on the PC |
| Phases 82–88: Android and iOS | **Deferred** by policy; every criterion marked so |
| Phases 89–93: trim and tighten (engine hot-patching out and scripts hot reload with their state kept, like s&box (changed 2026-11-12); narrower Model Studio, one graph canvas, legacy paths, a multithreaded frame) | Phase 89 **done** (scripts keep their top-level locals across a save in Play, [ADR-0023](../Architecture/Architecture-Decisions/ADR-0023-Scripts-Keep-State-Across-Hot-Reload.md); engine hot-patching and its `subsecond`/`dioxus` crates gone, `engine-dev` keeps Rebuild and Relaunch). Phase 90 **done** (row above). Phase 91 **done**: the node-graph views, the block Graph view and the Animation state machine draw on one canvas, `editor-node-graph/src/canvas.rs`, with the same keys in all five views; its line-count goal (−800) wasn't met. Phase 92 **done**: project format 6 (`project/src/migrate_shapes.rs`), the legacy readers and the Starter template gone, `animation:Play()` an error naming `LoadAnimation`, and the plan's built part moved to `plans/axcenity-engine-unified-done.md`. Phase 93 **built** (thread "Finish unchecked items in unified plan", 2026-11-12): skeletal posing on the animation pool (`anim/src/threads.rs`, `AXCENITY_ANIM_THREADS`), the Profiler's "Posing by thread" and the crowd bench scene (`axcenity-test --make-crowd`), on top of parallel physics (M3), the renderer's planning pool and the nav worker (M13); its 30% desktop target is measured on the PC. Phase 92 bumps the format to 6, since 5 is the release system's engine record |

Graphics plan: Phases 1–38 built.

## Editor usability and design system

| Work | Status |
|---|---|
| Usability items U1–U20 | **Done** (#31–#56) |
| Design system V1–V26 in six phases | Phases 1–5 **done** (#66 tokens; #71 feedback: errors stay until seen, one way to open and close windows; #73 scale: Explorer and Console draw only what's visible; #74 editing: Properties edits every selected item, Explorer arrow keys; #79 shortcut table, layout presets, Increase contrast); Phase 6 (optional polish) **planned** |
| Universal search (Ctrl+K) with indexed providers | **Done** (#76) |

## Cohesion pass (brief of 2026-11-12)

Audit and plan: `docs/research/cohesion-pass.md` (findings CF1–CF22, phases CP1–CP9), [ADR-0024](../Architecture/Architecture-Decisions/ADR-0024-Shared-Editor-Foundations-For-The-Cohesion-Pass.md).

| Phase | Status |
|---|---|
| CP1 audit and plan | **Done** |
| CP2 shared UI foundation, start-screen pages, welcome page | **Done** (#229) |
| CP3 documents and saving, open action, script views | **Done**: saving #228, Explorer double-click and Code/Blocks for scripts inside objects #230 |
| CP4 node catalogue and tooltips | **Done** (#231): catalogue and one tooltip card; shared Diagnostic (CW4.3) deferred |
| CP5 Object Viewer | **Done** (#232): one tab for Explorer objects, models, prefabs and materials |
| CP6 physics module split and properties | Done (#233): `physics::api`, drag and rolling friction, ApplyForce/ApplyTorque, momentum |
| CP7 Team Projects, trust, firewall | Done (#234): Team Projects start page, Who gets in (ask the host), narrower listening, firewall check, How Collaborate works |
| CP8 game branding | Done (#235): the manifest's icon and banner with checks, Settings > This project > Branding, the player's game icon, Build Game's `.ico`/`.icns`/PNG, `branding://` paths |
| CP9 final validation | Done: every crate's tests on main with CP7 and CP8 (only the no-audio tutorial test fails in the cloud), clippy, wiki check, snapshot review (the Object Viewer hint now wraps at 260 pt); hand checks listed in each PR |

## Tooling for AI clients and releases

| Work | Status |
|---|---|
| MCP toolkit: `axcenity-editor --mcp` | **Done** ([MCP server](../Editor/MCP-Server.md); #97 design, #102–#109, #116, #119, #123, #127, #130 incl. `--mcp --check` and a user guide) |
| Release system: one version source, release history, tag-only signed draft releases, Version Explorer and update checks, the updater, project format 5 | Phases 3–7 **done** ([Release process](../Development/Release-Process.md); #98, #105, #113, #118, #124, #129). Next release `0.2.0-alpha.1` |

## Deferred by policy (revisit points)

| Capability | Revisit when |
|---|---|
| CI on pushes and pull requests | Architecture and core systems are stable |
| Cross-machine determinism | Lockstep or deterministic replay is actually planned |
| Mobile and web | The desktop engine is reliable |
| Internet-hosted servers, secure netcode (M11) | Local multiplayer and server/client fundamentals are mature |
| Native `vector` as Vector3's representation | Its type-identity and interop checks pass |
