---
title: Documentation history
type: reference
status: current
verified: 2026-11-10
tags: [history, meta]
---

# Documentation history

Every change to this wiki, newest first: one line each. Keep the five newest also on the [Home](../Home.md#recently-updated) page.

| Date | Pages | Change | By |
|---|---|---|---|
| 2026-11-12 | System inventory | Collaborate C3: `collab::tailscale` (status, sign-in, invite address, reachability); the system's new name | Claude |
| 2026-11-12 | ADR-0021, ADR-0020, ADR index, roadmap | Collaborate audit (C1): ADR-0021 supersedes ADR-0020's D3 (live `yrs` scripts), TC8 (identity seam, no hosted service) and the name; Tailscale as the guided connection | Claude |
| 2026-11-12 | Scripting, scene bridge, technical debt | Usability brief Phase 6: `ScriptRuntime` methods split into `runtime/`, the draw list into `draw_list/`; oversized-files and crate-`CLAUDE.md` debt rows | Claude |
| 2026-11-12 | MCP server | Toolbox tools: `toolbox_search`, `toolbox_get` | Claude |
| 2026-11-12 | MCP server | `data_edit`: data tables for AI clients | Claude |
| 2026-11-12 | MCP server | `view_capture`: pictures as MCP image content | Claude |
| 2026-11-12 | MCP server | Step 7 measurements and the TCP_NODELAY fix for live clients | Claude |
| 2026-11-12 | MCP server | Small-model results and lenient arguments (lists and numbers sent as text) | Claude |
| 2026-11-11 | Editor architecture, Toolbox | Plugin toolkit: `.axcenity-plugin` packs, scaffold/export, Toolbox Import plugin; start-screen rail crossfade and atmosphere | Cursor |
| 2026-11-11 | MCP server | `render_stats` tool for the open editor | Claude |
| 2026-11-11 | MCP server, threading model | MCP step 3.4: clients work in the open editor (Settings ▸ AI connections, loopback listener, relay, shared change log); the page also caught up with steps 4–6 (#116–#133) | Claude |
| 2026-11-11 | System inventory, roadmap | Team Create TC2: the World's edit journal, `collab::capture`, the editor's session (`app/team.rs`) | Claude |
| 2026-11-11 | System inventory, system dependencies, roadmap | Team Create TC1: the `collab` crate (sync core) | Claude |
| 2026-11-11 | Repository structure | `documentation/` is now the user documentation (Learn, reference, generated API pages), embedded in the editor | Claude |
| 2026-11-11 | Dependencies, rendering, roadmap, release process, project format, audio, profiling | Caught up with PRs #116–#130 (M12, format 5, G5, G7, G8, updater); moved the vault into the repo as `wiki/` with personal details removed | Claude |
| 2026-11-11 | 30+ pages; new: Gameplay systems, MCP server, Toolbox, Release process, ADR-0018, ADR-0019 | Caught up with PRs #65–#115 merged while the wiki was built (physics pools and interpolation, L7, G1–G3, MCP, releases, design system Phases 2–5, Farmhouse closed, CI policy change) | Claude |
| 2026-11-10 | Asset pipeline, performance, roadmap | Updated for PR #65 (embedded cooking) and PR #66 (design tokens), which merged during the build | Claude |
| 2026-11-10 | Whole wiki | Created from the repository audit at `f47d5f0` (Phase 1 inventory, architecture, 20 system pages, editor, asset pipeline, performance, 17 ADRs, development guides, reference, roadmap) | Claude, for Paul |

## Open documentation gaps

Found while building the wiki. Fix them in the repository docs (or here), then strike them from this list.

| Gap | Where | Fix |
|---|---|---|
| ~~`docs/architecture/crates.md` has no row for `sim`; the `player` and `editor` rows omit it; `testing`'s listed dependencies are now reached through `sim`~~ | repo docs | Fixed on the wiki's branch (`claude/project-thread-xtwojp`) |
| ~~`repo-gotchas.md` says `unsafe_code = "forbid"`~~ | repo skill | Fixed on the same branch (`deny`, with the debugger exception) |
| ~~L1's measurement wasn't recorded in the repo docs~~ | repo docs | Fixed upstream by PR #87 |
| The modernization report's subsystem map says "clustered lights"; the renderer tiles lights in 2D (`docs/rendering/lights.md`) | repo research doc | Note the correction (research docs are snapshots; a footnote is enough) |
| `docs/rendering/performance.md`'s Baseline says `build_draw_list` 3.3 ms for 10,000 parts; it's 8–9 ms since before `556b72b` (noted in `docs/benchmarks.md`), unexplained | repo docs | Re-baseline or bisect |
| ~~`benchmarks/baselines.json` held pre-#65 Farmhouse numbers~~ | repo data | Re-recorded at `15965da` by PR #70 |
| `documentation/` and `gitbook-docs.yaml` are a placeholder GitBook scaffold (`# Page`) | repo | Remove or populate |
| No wiki page yet for the release workflow's packaging internals, the editor updater and Version Explorer, the cross-engine benchmark harness (`benchmarks/cross-engine/`), the LOD Inspector and `profile_project`/`OverdrawStats` tools | wiki | Add pages when those systems are next changed |
| Per-subsystem deep dives not yet written in this wiki: the UI Designer internals, terrain generation and erosion algorithms, the material graph compiler, the audio mix graph, the plugin API surface, the developer console | wiki | Add pages when those systems are next changed |
| Obsidian follows `#heading` anchors written as GitHub slugs only partly; links still open the right page | wiki | Accepted trade-off for GitHub and agent portability |
