---
title: "ADR-0015: Vector3 stays a userdata until native vector passes its checks"
type: adr
adr: 15
adr_status: Proposed
date: 2026-11-10
affects: [script, blocks, net, save, definitions]
source: [crates/script/src/types.rs, crates/script/src/native_vector_tests.rs]
docs: [docs/development-policy.md, docs/research/native-vector-check.md]
tags: [adr, scripting]
---

# ADR-0015: Vector3 stays a userdata until native vector passes its checks

- **Status:** **Proposed** (the switch is on hold). Policy set 2026-11-10; the check list is in `docs/research/native-vector-check.md` and `crates/script/src/native_vector_tests.rs`.
- **Affected systems:** scripting types, RemoteEvents, attributes, saves, luau-lsp definitions, the blocks translator

## Context and problem

Every Vector3 result allocates an mlua userdata (~980 instructions per read; L3 measured no gain from native code on Vector3 math). Luau's native `vector` type would remove that, but `type()` returns `"vector"`, field casing and methods differ, and interop across RemoteEvents, attributes, saves and blocks must hold.

## Decision (proposed)

Keep `Vector3` as engine userdata with a Roblox-familiar API (`typeof(v) == "Vector3"`, `.X/.Y/.Z`, `Magnitude`, `Unit`, `:Dot`, `:Cross`, `:Lerp`). Adopt native `vector` only after the checks pass: `typeof`/`type`, field casing, methods through the type metatable, equality and table keys, RemoteEvents, attributes, saves, definitions, blocks output. A compatible public API may sit over a faster internal representation.

## Consequences

- Vector-heavy scripts keep their allocation cost; `--!native` doesn't help engine calls.
- Accept (and implement) this ADR's alternative only with the checks green; then mark it Superseded by the new ADR.

## Implementation references

`docs/research/native-vector-check.md`; `crates/script/src/types.rs`; [Scripting](../../Engine-Systems/Scripting.md).
