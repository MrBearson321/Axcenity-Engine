---
title: Engineering changelog
type: reference
status: current
verified: 2026-11-10
verified_commit: 19c7179d
tags: [history]
---

# Engineering changelog

Architecture-level changes, newest first. Not every commit: the ones that changed how the engine is built, how it performs, or a decision. Git history (`git log --first-parent origin/main`) and PR descriptions have the rest.

## 2026-11-11

- **Team Create** design approved (#134, ADR-0020), TC1 sync core: the `collab` crate (#140). TC2 editor sync through the World's edit journal (#150). TC3 network and identity (Ed25519 device keys, single-use invite links, WebSocket over TLS 1.3 to a pinned certificate, #154). TC4 roster, presence and the Team Create panel (#156, #160). TC5 files: saves by SHA-256 with compare-and-swap, uploads and fetches in 1 MiB pieces, document locks, mirror folders with conflict copies (#168), then the editor side: the host serves its project folder, a joiner's folder opens as the project, read-only tabs others are editing, imports as one step (wire format 2, handshake protocol 4, #169). Then every document tab locks while edited (block scripts, UI Event Graphs, behavior trees, dialogues, data tables, materials), what a teammate saves is read again (`strings.json`, `input.json`, UI graphs too), and the start screen gets Join… (#170). TC6 reliability, part one: the host journals a snapshot and every accepted transaction in `.axcenity/teamcreate` (a fresh snapshot every 30 s), a hosting editor that crashes gets the session's World back when the project opens, others' edits count as changes to save for the host, and members are held to 32 MiB a message and 1,000 messages a second. TC6 part two: host epochs (a member never resumes from a restarted host's log, and re-sends its edits with IDs from its new block), a fix for a restarted editor's first edits being dropped, members' recovery files and the Recovered Changes window, read-only copies when the host ends the session, following a member's camera, and Settings > Team Create (wire format 3, protocol 5).
- **M12** wgpu 30, egui 0.36, egui_dock 0.21, glyphon 0.12 (#104). **Project format 5** (#129). **Updater** and **Version Explorer** (#124, #118). **G5 UI prefabs** (#122), **G7 memory breakdown** (#125, #126), **G8 audio roll-off and occlusion** (#128), G2 on players' devices (#117). rapier patched in `vendor/` so Ice and Rubber stacks stand (#121). MCP steps 4–6 (#116, #119, #123, #127, #130).
- **G2 scripts inside objects** (#91 design, #111 format, loading and runtime, #112/#115 editor support, #114 asset references, Asset Check, packages, Build Game).
- **G3 physics render interpolation** (#110). **Physics step part 1** (#84), **own thread pool** (#86, fixed by #93), **render planning pool** (#88), **contact recycle 0.1 studs** (#99).
- **G1 Toolbox** (#90, #92, #96, #107): `toolbox` crate, ambientCG/Poly Haven/Openverse, central asset directory, CLI, Python and Blender providers.
- **MCP toolkit** (#97 design, #102, #103, #106, #108, #109): `mcp` crate, `axcenity-editor --mcp`.
- **Release system** Phases 3–4 (#98, #105, #113): one version source (`0.2.0-alpha.1`), `release` crate, `axcenity-release`, tag-only signed draft releases; policy: Actions for release tags only.
- **L7 gameplay crate** (#68, #72, #77, #82, #85). **L2 references by asset ID** (#78). **L1 measured** recorded (#87).
- **Design system Phases 2–5** (#71, #73, #74, #79); **universal search** (#76); recent projects grid (#75).
- **Farmhouse startup closed** (#67, #69, baselines #70). **XB10k cross-engine benchmark** (#80, #94), **engine comparison** (#95), **gap analysis and G roadmap** (#89). Rendering investigations (#83, #100). macOS Build Game signing (#81). Repository made public; `SECURITY.md` added.

## 2026-11-10

- **PR #65** Cook textures embedded in models in place, one shared copy per image per project (Farmhouse first frame 2.9 → 1.9 s, GPU textures 947 → 311 MB).
- **PR #66** Editor design system Phase 1: finished tokens (graphs follow the theme, radius tokens, one overlay palette, changed-value mark, graph zoom).
- **PR #64** Windows performance baselines and `axcenity-bench-check`; **PR #62** package and Build Game checks per OS without CI (`scripts/check-packages.py`); **PR #63** editor design-system audit (DS1–DS27, V1–V26).
- **PR #61** M9: `.axgame` v2 (zstd per entry, mtimes, threaded unpack).
- **PR #59** L4 first measurement → kept query bounds; L1 measured and parked. **PR #58** L3 part 1: `safeenv`, read-only libraries, `--!native`.
- **PRs #45–#57** L2 import pipeline: cooked BC7 textures, shipped in builds, LOD cache, asset IDs, cooked models, orphan cleanup, cook on change, BC5 normal maps.
- **PRs #31–#56** Usability pass U1–U20 (Animation, Properties, Explorer, Terrain Studio, UI Designer, Block editor, Model Studio).
- **PR #37** M14 UI layout cache. **PR #34** M7 postcard wire and motion revisions (protocol 10). **PR #32** M13 navmesh worker. **PR #25** M3 physics sync + rapier `parallel`. **PR #23** M4 retained draw items. **PRs #22, #26, #27, #29** M6 script hot paths. **PR #21** kept MeshPart hulls, no mouse ray without ClickDetectors. **PRs #18–#19** M5 assets off the lock, non-blocking draws. **PR #17** M2 class and tag index. **PRs #15, #16, #20, #24** M1 shared frame schedule (`sim` crate).
- **PR #28** Development policy: CI off on purpose, desktop first, determinism not required, native `vector` on hold, internet hosting deferred.
- **PR #14** Engine modernization report and roadmap (I/M/L).
- Engineering wiki created (this vault).

## 2026-11-09

- **PRs #4–#13** Tier 1: I2 Instance table keys, I1 measure first (S10k, `--bench`, spans), I3/I5/I6 pins and profiles, I4 cheap `with_world`, I11 player preload, I10 `notify` watcher, I8 shared MeshPart meshes, I9 `descendants_iter`/Fx, I7 navmesh from anchored ground, M8 server without GPU/audio.
- `3f2fc05` Threaded draw planning, cut-out depth fill, alternating shadow cascades. `556b72b` Leaf crowns and gentler real-time GI in heavy scenes.

## 2026-10-17 → 2026-11-08 (plans v1, v2, v3, unified, graphics)

| Dates | Work |
|---|---|
| 2026-10-17 | v1 plan: workspace, player window, parts, the Instance arena, PBR from the first frame |
| 2026-10-17 → 10-20 | v1 phases: editor shell, Play and Stop (Phase 12), deferred change events (Phase 16), Lua 5.4 runtime |
| 2026-10-20 | v2 plan: characters, navigation, saves, editor-only tools |
| 2026-10-22 | v3 plan: graphs, console, one clip format (project format 3) |
| 2026-10-25 | **Luau replaces Lua 5.4** ("Luau Tooling"); unified plan of the remaining v2/v3 work |
| 2026-10-26 → 10-29 | Graphics plan Phases 1–38: settings and presets, AA, AO, culling, LOD, GPU skinning, environment maps, probes, real-time GI, material editor, surface shaders, material graphs, atmosphere, fog, clouds, weather, wind, water, toon, outlines, grading, volumes, lens, TAA, GPU particles, profiler, debug views |
| 2026-10-29 → 11-08 | Unified Phases 34–80: physics depth, gameplay kit, levels, prefabs, CSG, sequencer, audio buses, game tests, plugins, Model Studio (55–62), networking (63–67), templates, command palette, developer console, version history, splines, decals, abilities, dialogue and quests, animation layers, runtime IK, world streaming |
| 2026-11-08 | Painted forests: foliage groups, distance bands, translucent leaves; blueprint parts; any Luau as blocks and graphs |
