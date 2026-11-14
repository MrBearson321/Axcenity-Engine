---
title: Architecture decisions
type: index
status: current
verified: 2026-11-10
tags: [adr, index]
---

# Architecture decisions

Decisions with long-term consequences, so later work doesn't silently reverse them. Write a new ADR from `Templates/ADR.md`; never edit an accepted ADR's decision in place: supersede it with a new one and mark the old one **Superseded by ADR-NNNN**.

**Before changing code that touches one of these, read it.** If a change conflicts with an accepted ADR, say so in the plan or pull request and either fit the change to the decision or propose a superseding ADR for Paul to accept.

| ADR | Decision | Status | Date |
|---|---|---|---|
| [ADR-0001](ADR-0001-Layered-Crate-Workspace.md) | Layered crate workspace with forbidden edges (player has no editor code; server has no GPU/audio/window) | Accepted | 2026-10-17 |
| [ADR-0002](ADR-0002-Instance-Tree-In-A-Generational-Arena.md) | Instance tree in a generational arena, not an ECS | Accepted | 2026-10-17 |
| [ADR-0003](ADR-0003-Renderer-Consumes-Only-Draw-Data.md) | The renderer consumes only draw data | Accepted | 2026-10-17 |
| [ADR-0004](ADR-0004-Luau-As-The-Only-Script-Runtime.md) | Luau is the only script runtime; blocks and graphs compile to Lua | Accepted (supersedes v1's Lua 5.4) | 2026-10-25 |
| [ADR-0005](ADR-0005-Deferred-Change-Events.md) | Deferred change events with a 10-round cap | Accepted | 2026-10 (v1 Phase 16) |
| [ADR-0006](ADR-0006-One-Shared-Frame-Schedule.md) | One data-driven frame schedule for every host | Accepted | 2026-11-10 |
| [ADR-0007](ADR-0007-Fixed-Step-Physics-And-Kinematic-Characters.md) | Fixed 60 Hz physics; kinematic characters | Accepted | 2026-10-20 |
| [ADR-0008](ADR-0008-Desktop-Texture-Cooking.md) | Cook assets offline into per-machine `.axcenity/imported`, desktop formats | Accepted | 2026-11-10 |
| [ADR-0009](ADR-0009-No-Render-Graph-Yet.md) | No render graph yet | Accepted | 2026-10 |
| [ADR-0010](ADR-0010-Listen-Server-Replicas-And-Network-IDs.md) | Server-authoritative replicas with shared IDs; single-player is a listen server | Accepted | 2026-10-29 |
| [ADR-0011](ADR-0011-Desktop-First-Local-Verification.md) | Desktop first, verified locally, no CI for now | Accepted (supersedes v1's CI rule); its "Actions off" part amended by ADR-0018 | 2026-11-10 |
| [ADR-0012](ADR-0012-Wiki-Complements-Topic-Docs.md) | The wiki explains and links; topic docs stay the per-change reference | Accepted | 2026-11-10 |
| [ADR-0013](ADR-0013-Look-Versus-Cost-Settings.md) | Look belongs to the world, cost to the machine | Accepted | 2026-10-26 |
| [ADR-0014](ADR-0014-Determinism-Not-Required.md) | Cross-machine determinism is not a requirement | Accepted | 2026-11-10 |
| [ADR-0015](ADR-0015-Native-Vector-On-Hold.md) | Vector3 stays a userdata until native `vector` passes its checks | **Proposed** (on hold) | 2026-11-10 |
| [ADR-0016](ADR-0016-Text-Project-Format-With-Migrations.md) | Text project files, deterministic, versioned with backed-up migrations | Accepted | 2026-10-17 |
| [ADR-0017](ADR-0017-Editor-Play-In-Process-On-A-Cloned-World.md) | Editor Play runs in-process on a cloned World | Accepted | 2026-10 (v1 Phase 12) |
| [ADR-0018](ADR-0018-Actions-For-Release-Tags-Only.md) | GitHub Actions runs for release tags only; CI by hand | Accepted (amends ADR-0011) | 2026-11-11 |
| [ADR-0019](ADR-0019-Gameplay-Systems-Through-GameSystem.md) | Gameplay systems in their own crate behind `GameSystem`; Players stays in `script` | Accepted | 2026-11-11 |
| [ADR-0020](ADR-0020-Team-Create-Session-Authority-And-Operations.md) | Team Create: one session authority ordering typed operations; device-key identities until accounts exist | Accepted; D3, TC8 and the name superseded by ADR-0021 | 2026-11-11 |
| [ADR-0021](ADR-0021-Collaborate-Tailscale-Live-Scripts-And-Identity-Seam.md) | Collaborate: Tailscale as the guided connection, Lua scripts as live `yrs` documents relayed by the host, an identity-provider seam instead of a hosted account service | Accepted | 2026-11-12 |
| [ADR-0022](ADR-0022-Persistent-Collaborate-Projects-Git-Sync-And-Host-Handoff.md) | Persistent Collaborate projects: the session host is replaceable by a deterministic successor, ownership stays in the shared members list, optional Git remote for checkpoints | Accepted | 2026-11-12 |
| [ADR-0023](ADR-0023-Scripts-Keep-State-Across-Hot-Reload.md) | Saved scripts keep their top-level locals across a reload in Play (s&box-style); engine hot-patching removed | Accepted | 2026-11-12 |
| [ADR-0024](ADR-0024-Shared-Editor-Foundations-For-The-Cohesion-Pass.md) | The cohesion pass extends existing foundations (kit, two scripting IRs with one node catalogue, physics `api` module boundary, Collaborate trust layer, shared preview canvas) instead of building parallel ones | Accepted | 2026-11-12 |
| [ADR-0025](ADR-0025-Optional-Ray-Tracing-Through-Ray-Queries.md) | Hardware ray tracing is an optional tier built on wgpu's ray queries: a BLAS per mesh and a TLAS only while an effect traces rays, each effect its own setting, off at every preset | Accepted | 2026-11-12 |
| [ADR-0026](ADR-0026-Secure-Game-Connections-And-Relay.md) | Game servers issue their own netcode connect tokens over TLS pinned by fingerprint (LAN and Play stay unsecured); reaching a host goes direct, port mapping, hole punching, then a self-hostable `axcenity-relay` | Accepted | 2026-11-14 |
| [ADR-0027](ADR-0027-Convert-New-Model-Formats-To-glTF-On-Import.md) | Model formats the runtime doesn't read (USD first) are converted to `.glb` by the editor-only `convert` crate on import; OBJ stays direct through `ufbx` | Proposed | 2026-11-14 |
| [ADR-0028](ADR-0028-Facial-Animation-Morph-Targets-On-GPU-And-FaceControls.md) | Blend shapes morphed on the GPU before skinning (sparse deltas per mesh, weights per item); faces driven by a `FaceControls` Instance of standard controls, mapped to shapes and bones by `*.face.json` profiles found by fit (Axcenity, ARKit, VRM, Oculus visemes) | Accepted | 2026-11-14 |

## Superseded and changed decisions found in the plans

Earlier decisions that later work replaced. They live in the plans' "Architectural decisions" sections and topic files; this table keeps them visible so nobody re-applies the old one.

| Original decision | Where | Replaced by | When |
|---|---|---|---|
| `mlua` with **Lua 5.4** (vendored) | `plans/axcenity-engine-v1.md` | Luau ([ADR-0004](ADR-0004-Luau-As-The-Only-Script-Runtime.md)) | 2026-10-25 |
| "CI builds and tests all three OSes from Phase 1; a phase isn't done unless CI is green" | `plans/axcenity-engine-v1.md` | Local verification ([ADR-0011](ADR-0011-Desktop-First-Local-Verification.md)) | 2026-11-10 |
| Windows, macOS and Linux as equal targets | v1–v3 plans | Windows and Linux primary, macOS secondary, mobile and web deferred ([ADR-0011](ADR-0011-Desktop-First-Local-Verification.md)) | 2026-11-10 |
| Behavior trees with their own graph editor | `plans/axcenity-engine-v2.md` | One generic node-graph editor for UI Event Graphs and behavior trees (unified plan, "Changed") | 2026-10-25 |
| `.axpkg` needs a new zip dependency | `plans/axcenity-engine-v3.md` | Reuses the pinned `zip` (unified plan, "Changed") | 2026-10-25 |
| File watching by polling, not `notify` | `docs/editor/scripts.md` (earlier) | `notify` events with a polling fallback (I10, PR #9) | 2026-11-10 |
| Hot-patching engine code with `subsecond` and `dx` | unified Phase 26 | Removed; saved scripts keep their state instead ([ADR-0023](ADR-0023-Scripts-Keep-State-Across-Hot-Reload.md), Phase 89) | 2026-11-12 |
| Each host hand-orders its frame from `frame-loop.md` | v1–v3 | One schedule ([ADR-0006](ADR-0006-One-Shared-Frame-Schedule.md)) | 2026-11-10 |
| Reliable network messages and snapshots in JSON | Phase 63–64 | postcard (M7, protocol 10); `Hello`/`Refused` stay JSON | 2026-11-10 |
| Roadmap L1 (typed property slots) "do after M2 and M4" | modernization report | Measured and parked: name lookups ~9% of S10k (PR #59, recorded in PR #87) | 2026-11-10 |
| GitHub Actions disabled entirely | development policy 2026-11-10 | Release tags only, CI by hand ([ADR-0018](ADR-0018-Actions-For-Release-Tags-Only.md)) | 2026-11-11 |
| References name assets by path only | L2 | References may also name the asset ID (`ax://…`), resolved by `Assets::resolve` (PR #78) | 2026-11-11 |
| Scripts only as files under `scripts/`, never in `world.json` | v1 Phase 4 | Scripts may also sit inside objects as `SourceFile` references, usually in `objects/` (G2, PRs #111, #114, #115) | 2026-11-11 |
| No physics interpolation | physics docs | Moving parts drawn between steps (G3, PR #110) | 2026-11-11 |

## Candidates not yet written

Worth an ADR if they come up for change: the RemoteEvent wire and side rules (`sides.rs`), the editor design-system tokens and shortcut table (design-system Phases 1–5), scripts inside objects as file references (G2; `docs/research/scripts-in-objects.md` records Paul's approval), the MCP server's editor-only, job-based shape (`docs/research/mcp/architecture.md`), the Toolbox's central asset directory (G1), and the editor crate split (L6/G11) when it starts.
