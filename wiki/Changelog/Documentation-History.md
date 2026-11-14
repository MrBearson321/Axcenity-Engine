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
| 2026-11-14 | Animation, roadmap | F6: the Character Studio tab edits faces (profile, controls, shape mapping, expressions, lip-sync timeline) | Claude |
| 2026-11-14 | ADR-0028, ADR index, roadmap, Animation | Facial animation plan F0–F7 (`docs/research/facial-animation.md`) | Claude |
| 2026-11-14 | Supported formats, importers | P7.1: OBJ models with their `.mtl` read through `ufbx` | Claude |
| 2026-11-14 | Supported formats, importers, dependencies, system dependencies, file formats | P7.2: USD converted to `.glb` on import by the editor-only `convert` crate (`openusd`); `converted_from` in sidecars | Claude |
| 2026-11-14 | ADR-0027, ADR index, roadmap | P7 investigation: USD, Alembic and OBJ import, convert-on-import, mesh import presets, plugin API 3 (`docs/research/import-formats-and-plugins.md`) | Claude |
| 2026-11-14 | Lighting and atmosphere, roadmap | Runtime lighting parity audit (`docs/research/runtime-lighting-parity.md`): what played games get of ray tracing, items RL1–RL5 | Claude |
| 2026-11-13 | Rendering, lighting and atmosphere | Volumetric fog: own light by openness, 8 shadow samples a cell, dust boxes (FogEffect `Density`); ray traced shadows through cut-outs' holes (`ray_cut_outs.rs`) | Claude |
| 2026-11-12 | Roadmap | Cohesion pass CP9: final validation done (CP7 and CP9 marked done) | Claude |
| 2026-11-12 | Project and serialization, roadmap | Cohesion pass CP8: the game's icon and banner (`branding.rs`), used by the player, Build Game and `branding://` paths | Claude |
| 2026-11-12 | System inventory, roadmap | Cohesion pass CP7: Team Projects start page, Who gets in (the host asked about joiners), hosts listening on loopback and their network only, firewall checks | Claude |
| 2026-11-12 | Physics, roadmap | Cohesion pass CP6: physics layers (`physics::api`), drag and rolling friction, script forces; `step_cost` at 1k/5k/10k bodies | Claude |
| 2026-11-12 | Editor architecture, roadmap | Cohesion pass CP5: Object Viewer (`editor-tools/src/object_viewer.rs`), opened by View in Explorer and Assets | Claude |
| 2026-11-12 | Editor architecture, roadmap | Cohesion pass CP4: node catalogue (`blocks-runtime/src/catalogue.rs`) and one tooltip card for every visual scripting view | Claude |
| 2026-11-12 | Editor architecture, roadmap | Cohesion pass CP3: Explorer double-click opens each Instance in its own editor or frames it in the viewport; Code ↔ Blocks and Blocks tabs for scripts inside objects (`plan_switch`) | Claude |
| 2026-11-12 | Editor architecture, roadmap | Cohesion pass CP2: category hues (`theme::Hue`) on the ribbon, one menu button and menu-item shape (Menu icons setting), ribbon caption baseline, Console and Blocks bar overflow, reading glass only on reading pages, the Welcome tab when no document is open | Claude |
| 2026-11-12 | Roadmap | Cohesion pass, saving (CW3.1): one list of modified documents (`app/documents.rs`); Don't Save puts back clips, state machines and sequences written since the save and drops unwritten clip and Model Studio changes | Claude |
| 2026-11-12 | ADR-0024, ADR index, roadmap | Cohesion pass CP1: audit of the design system, scripting IRs, physics, Collaborate, previews, saving and branding (`docs/research/cohesion-pass.md`), and the decision to extend existing foundations | Claude |
| 2026-11-12 | ADR-0021, inventory, roadmap | Collaborate C12: the future Django account integration as a document (`docs/research/collaborate-accounts.md`): providers, token checks, linking members, ownership, the website's API, threat model | Claude |
| 2026-11-12 | Optimization log, benchmark history, performance overview, known bottlenecks, project and serialization, known limitations, ADR-0016, inventory, roadmap | Collaborate C11: zstd for big messages, S10k session measurements, and the quadratic `world.json` load fixed (S10k 5.66 → 0.19 s) | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate C10: `IdentityProvider` seam, per-member budgets for each message kind and handshake limits (`crates/collab/src/limits.rs`), the host's audit log (`audit.rs`), security review in `docs/research/collaborate.md` §6.1 | Claude |
| 2026-11-12 | ADR-0023, ADR index, editor architecture, dependencies, configuration, coding standards, roadmap | Unified Phase 89: engine hot-patching removed (`engine-dev` feature for rebuild and relaunch); saved scripts keep their top-level locals across a reload in Play | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate 2.0 P7: checkpoints when a member leaves, hosting passes on or the session ends; a successor's copy takes up the project's history; scenarios A–E as editor tests (`collab_scenario_tests`, `collab_repo_tests`); the Collaborate guide covers repositories, handoff, rejoining, conflicts and offline work | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate 2.0 P6: the Collaborate panel's Project section (owner, host, rejoin, backup host, let others host), the repository's sync chip and buttons, and the Repository (GitHub sign-in, connect, create, disconnect), History (restore) and Resolve Conflicts windows (`crates/editor/src/app/collab_repo_ui.rs`) | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate 2.0 P5: a conflict writes the repository's version beside each file (kept out of checkpoints), per-file resolve (mine, theirs, both) as one merge checkpoint, its record in the repository's Git folder, unfinished resolves undone (`crates/repo/src/conflicts.rs`, editor `app/collab_conflicts.rs`, Console `conflicts`, `resolve`) | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate 2.0 P4: successors from host cards, the host handing the session on (Leave) or a successor taking over a lost host after the grace period and recovering its transactions from members' tails (`ToClient::Successors`, `HandingOff`, `SendTail`, `ToHost::HostCard`, `Tail`; wire format 9), End for everyone (`crates/collab/src/election.rs`, `host/handoff.rs`, editor `app/collab_handoff.rs`) | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate 2.0 P3: persistent project ID, the members list sent to whoever may host (`ToClient::Members`, wire format 8), project records, rejoin and hosting for members without an invite (`crates/collab/src/project.rs`, editor `app/collab_project.rs`) | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate 2.0 P2: automatic checkpoints and sync (`crates/repo/src/sync.rs`, `checkpoint.rs`, editor `app/collab_repo.rs`) | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate 2.0 P1: `crates/repo` (git runner, GitHub device sign-in, credentials in Git's store, linking), `collaborate.json` | Claude |
| 2026-11-12 | World streaming, optimization log, known bottlenecks | Phase 80: the streaming world bench scene (`--make-streaming`) and the server's per-frame passes over every part cut (frame 21.9 → 4.2 ms in the container) | Claude |
| 2026-11-12 | Networking | Phase 81: lag compensation (`LagHistory`, `RaycastParams.CompensateFor`, `Tool.LagCompensated`), protocol 12 | Claude |
| 2026-11-12 | Networking | Phase 81: replays (`ReplayService`, `.replay` files, `Playback`, `axcenity-player --replay`) and kill cams | Claude |
| 2026-11-12 | ADR-0022, ADR index, system inventory, roadmap | Collaborate 2.0 design (P0): host handoff by deterministic successor, members list replicated, persistent project ID, optional Git remote (`docs/research/collaborate-persistent.md`) | Claude |
| 2026-11-12 | Mesh editing and CSG, specialized editors, roadmap | Unified Phase 90: Sculpt mode, retopology and Bake removed from the Model Studio; creators sculpt and bake in Blender | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate C9: host checkpoints (`crates/collab/src/history.rs`, `.axcenity/teamcreate/history`, contents by hash) and the Owner's restore for everyone as one transaction | Claude |
| 2026-11-12 | Specialized editors, blocks and graphs, technical debt | Phase 91: one graph canvas (`editor-node-graph/src/canvas.rs`) under every graph view, with the same keys, menu, minimap and copy and paste | Claude |
| 2026-11-12 | System inventory, roadmap | Collaborate C8: session chat (host keeps the last 200 lines in memory, Instance paths as links, unread count on the chip) and others' cursors in live script tabs (`crates/editor/src/app/team_chat.rs`, `crates/editor/src/scripts/others.rs`) | Claude |
| 2026-11-12 | Project and serialization, file formats, change a file format, technical debt | Phase 92: project format 6 (`migrate_shapes.rs`), old clip and block-condition readers out, `animation:Play()` an error, Starter template gone | Claude |
| 2026-11-12 | System inventory | Collaborate C4: the setup wizard and Start Session dialog (`app/collab_setup.rs`) | Claude |
| 2026-11-12 | System inventory | Collaborate C3: `collab::tailscale` (status, sign-in, invite address, reachability); the system's new name | Claude |
| 2026-11-12 | System inventory, ADR-0021, roadmap | Collaborate C7: live scripts and stylesheets (`collab::live`, `yrs` with `sync`), documents kept across host restarts, own-edit undo | Claude |
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
