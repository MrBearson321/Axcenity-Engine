---
title: "ADR-0004: Luau is the only script runtime; everything visual compiles to Lua"
type: adr
adr: 4
adr_status: Accepted
date: 2026-10-25
supersedes: "v1 plan: mlua with Lua 5.4 (vendored)"
affects: [script, blocks-runtime, blocks, project, editor]
source: [Cargo.toml, crates/script/src/luau_pin_tests.rs, crates/blocks-runtime/src/translate.rs, crates/project/src/ui_graphs.rs]
docs: [docs/scripting/language.md, docs/scripting/blocks.md, docs/scripting/graphs.md, plans/axcenity-engine-v1.md, plans/axcenity-engine-graphics.md]
tags: [adr, scripting]
---

# ADR-0004: Luau is the only script runtime; everything visual compiles to Lua

- **Status:** Accepted. The runtime choice changed on **2026-10-25** ("Luau Tooling", `be34c7b`): it **supersedes** the v1 plan's "mlua (Lua 5.4, vendored)". The "blocks translate to Lua, one runtime" half dates from the v1 plan (2026-10-17).
- **Affected systems:** script, blocks, UI Event Graphs, behavior trees, dialogues, editor tooling

## Context and problem

Creators write Roblox-style Lua, and beginners use blocks and node graphs. Two runtimes (an interpreter for blocks, Lua for code) would mean two behaviors, two debuggers and two sandboxes. Lua 5.4 lacked the type annotations, compound assignment, `continue`, the luau-lsp tooling and the fast interpreter and native code generator creators expect from a Roblox-familiar language.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Lua 5.4 (v1) | Standard, vendored | No types or Roblox-familiar syntax; weaker tooling |
| LuaJIT | Fast | Lua 5.1 dialect; no Luau syntax; platform limits |
| A separate block interpreter | Direct block semantics | Two runtimes |
| **Luau via mlua, everything compiles to Lua** | One runtime, Roblox-familiar syntax, luau-lsp, `--!native` | Pins `luau0-src` to mlua's bytecode version; no wasm path for native code |

## Decision

Luau via `mlua` (feature `luau`, `luau-jit` for opt-in native code), sources built by `luau0-src` pinned `=0.22.0`. Block scripts, their Graph view, UI Event Graphs and behavior-tree/dialogue nodes all translate to Lua and run in `ScriptRuntime`. Material graphs are the one exception: they compile to WGSL, never to Lua, and gameplay graphs never compile to WGSL (graphics plan).

## Consequences

- `luau0-src` and `mlua` move together; `luau_pin_tests.rs` fails if they drift.
- Every creator-facing Lua API needs a matching block; every block maps to a real API.
- `from_lua` can turn any Luau into blocks without loss (grey Lua blocks hold what blocks can't say).
- Lua 5.4-only syntax (`goto`, `<close>`, bitwise operators) is not supported.

## Implementation references

`Cargo.toml` (`mlua`, `luau0-src` pins and comments); `crates/script/src/luau_pin_tests.rs`; `crates/blocks-runtime`; [Scripting](../../Engine-Systems/Scripting.md), [Blocks and graphs](../../Engine-Systems/Blocks-and-Graphs.md).
