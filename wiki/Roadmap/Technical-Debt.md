---
title: Technical debt
type: roadmap
status: current
verified: 2026-11-12
verified_commit: e1eea07
docs: [docs/research/engine-modernization-report.md, docs/research/usability/design-system.md]
tags: [roadmap, debt]
---

# Technical debt

Structural problems that make the engine harder to change. Each has a roadmap item or is a candidate for one.

| Debt | Why it hurts | Planned answer | Status |
|---|---|---|---|
| `editor` is one ~177k-line crate; `EditorApp` has ~118 fields | 5–6 s incremental rebuilds; weak internal boundaries | L6/G11: sub-crates behind an `EditorContext`, one panel family per PR | In progress: shared UI base in `editor-kit`; Toolbox, Graphics Benchmark, shared tools, Model Studio, node-graph editor and UI Designer in crates of their own |
| ~~`script` hosts most gameplay systems with ~20 `step_*`/`fire_*` entry points~~ | Was: hard to test systems alone | L7: `gameplay` crate behind `GameSystem` | **Done** 2026-11-11 ([Gameplay systems](../Engine-Systems/Gameplay-Systems.md)); Players stays in `script` by decision; `net` still talks to `ScriptRuntime` through many methods |
| String-keyed property maps | ~9% of S10k instructions; ~2 KB per Part (estimate) | L1 typed slots | Parked by measurement |
| wgpu 25 / egui 0.32 / winit 0.30 locked together | Blocks newer wgpu features (L5, G9) | M12 | In progress (PR #104) |
| Retained draw list still has a from-scratch path | Two code paths | M4 next step: spatial chunks, then remove it | Open |
| Hand-ordered render passes | Each new pass edits `ScenePipeline::draw` | L8 render graph | Deferred |
| Editor design-system drift | Inconsistent UI; editor cost with big scenes | Design-system phases | Phases 1–5 done; Phase 6 optional |
| Several graph canvases | Duplicate code (`node_graph.rs`, block scripts' Graph view, the Animation state machine) | Phase 91 / design system V24 "One graph canvas" | Done: one `canvas.rs` in `editor-node-graph` used by every view (the two editor views are only 20 lines shorter, 2,366 to 2,346, because they gained copy and paste, the shared keys and a minimap while their own canvas code went; what they repeated is now in one place) |
| Oversized files with several jobs | Hard to find code; more merge conflicts | Usability brief Phase 6 (`docs/research/usability/refactoring.md`): verbatim splits into parent + folder | Done for Properties, Settings, Material tab, Assets panel, `ScriptRuntime`, the scene draw list and the player; `render/renderer/scene.rs` waits for L8 |
| `editor`/`script` crate `CLAUDE.md` lines are file maps (~10k characters) | Every editor PR edits them; most merge conflicts start there | Move the file maps into `docs/` | Proposed |
| Legacy paths, old templates | Dead weight | Phase 92 | Planned |
| `crates.md` host rows drift from the manifests | Agents read wrong rules | Documentation fix | Fixed on the wiki branch (see [Documentation history](../Changelog/Documentation-History.md#open-documentation-gaps)) |
| Flaky GPU tests under contention | Noise in local verification | Diagnose `every_example_renders` black frames | Open |
| `.axgame` unpacks to a temp folder; games ship sources beside cooked copies | Extra disk writes; about twice the size | G12: cooked-only games, read the pack in place | Planned |
