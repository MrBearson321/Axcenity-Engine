---
title: Implementation patterns
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [crates/core/src/world.rs, crates/scene/src/retained.rs, crates/assets/src/loading.rs, crates/sim/src/host.rs, crates/script/src/lent.rs]
docs: [docs/architecture/data-model.md, docs/rendering/performance.md, docs/project/assets.md]
tags: [development, patterns]
---

# Implementation patterns

Patterns the engine uses again and again. Reuse them before inventing a new one.

| Pattern | Use it when | Examples |
|---|---|---|
| **Refer by ID, look up every time** | Anything that points at an Instance | `InstanceId` in Lua userdata, `Value::Instance`, network IDs |
| **Mutate through `World`** | Any change to the tree, properties, attributes, tags | `World::set_property`, `create`, `set_parent`, `add_tag` (keeps index, revisions, change log) |
| **Skip what didn't change (revisions)** | A derived cache of world data | Retained draw items (M4), physics sync (M3), replication (`revision_without_motion`), motion (`motion_revision`), query bounds (L4), navmesh `MeshBuilder` |
| **Per-service structure versions** | A reader of one service | Physics, streaming (`World::structure_version_of`) |
| **Index instead of walk** | "Every X in the game" | `World::all_of`, `tagged_iter` (M2) |
| **Queue in the runtime, apply in the host** | Scripts asking for side effects the runtime mustn't own | `SoundAction`s → `GameAudio`; `NetworkRequest`s → `Network`; shatters → physics; level loads, test requests, chat |
| **`HostIo` for what differs per host** | Frame steps needing input, sound, network or drawing | `crates/sim/src/host.rs` |
| **Plain data across crate boundaries** | Two crates need a type but shouldn't depend on each other | `core::sound`, `core::input`, `core::spline`, `render-types` |
| **Lend the World** | Calling into Lua with world access | `lent.rs`: swap into the VM's slot and back, guard on panic (I4) |
| **Tickets, not blocking** | Slow loads | `Assets::request_*` → `LoadTicket`; `model_slot` draws nothing until ready (M5) |
| **Worker computes, frame applies** | Heavy work that doesn't read the World | Navmesh assembly (M13), terrain chunks, CSG, cooking |
| **Ordered parallel join** | Data-parallel loops | `render/src/renderer/parallel.rs` (`map_parts`, above 4,096 items) |
| **Deferred events** | Reacting to changes | `fire_changes` ([ADR-0005](../Architecture/Architecture-Decisions/ADR-0005-Deferred-Change-Events.md)) |
| **Capability lowering with a note** | A feature some GPUs lack | `GraphicsSettings::effective` |
| **Targets only while used** | GPU resources for optional features | `FrameTargets` |
| **Report once** | Repeated failures (missing texture, bad shader) | `Assets::take_errors`, `take_shader_errors` |
| **Keep the last good version** | Live reload | `Assets::reload`, script reload during Play |
| **One undo step per user action** | Editor edits | `EditorApp::change`, `History::record_*` |
| **Write nothing while planning** | Checks and fix planners | `project::check_assets`, `plan_fix`, `plan_rename` |
| **Stamp and verify caches** | On-disk derived data | Cooked textures (length + mtime + hash), cooked models (source + `AXCENITY_ASSETS_SOURCE_HASH`) |
| **Prove equivalence with a randomized test** | Replacing a slow path with a cached one | `scene/src/retained_tests.rs` |
| **A golden file for an order or a format** | Something that must not change silently | `sim/src/golden.rs`, `definitions_file_is_up_to_date`, example round trips |

## Anti-patterns (each was found and fixed)

- Walking the whole Workspace every frame for "all X" (M2).
- Reading properties by name in a hot loop over every part (L4, M7).
- Rebuilding a derived structure from scratch each frame when revisions say nothing changed (M4, I8).
- Holding a global lock while decoding (M5).
- Allocating a fresh collection per walk (I9) or a fresh World per Lua call (I4).
- Hashing debug-formatted strings (I7).
- Building a convex hull per query (PR #21).
