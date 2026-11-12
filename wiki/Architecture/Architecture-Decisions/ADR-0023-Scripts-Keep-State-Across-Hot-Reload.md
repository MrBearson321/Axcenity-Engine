---
title: "ADR-0023: Saved scripts keep their state across a reload; the engine isn't hot-patched"
type: adr
adr: 23
adr_status: Accepted
date: 2026-11-12
affects: [script, blocks, editor]
source: [crates/blocks/src/hot_reload.rs, crates/script/src/hot_reload.rs, crates/script/src/modules.rs, crates/editor/src/play.rs]
docs: [docs/editor/scripts.md, plans/axcenity-engine-unified.md]
tags: [adr, scripting, editor, live-reload]
---

# ADR-0023: Saved scripts keep their state across a reload; the engine isn't hot-patched

- **Status:** Accepted (Paul, 2026-11-12: "keep hot reloading but make it more similar to that of s&box", and "hotpatch game files not engine files"; unified plan Phase 89)
- **Affected systems:** script runtime, block scripts' Luau reader, editor Play

## Context and problem

Saving a script during Play reran it from scratch (unified Phase 24): its handlers were disconnected, its threads dropped, and its top-level code ran again, so counters reset and parts it made were made twice. Engine Rust could be hot-patched with `subsecond` (Phase 26), but only with the editor started through `dx serve`, and Milestone 21 had already planned to remove that. s&box, which Paul named as the model, recompiles saved game code and swaps it into the running game in milliseconds, copying every object's field values into the new code ([hotloading](https://sbox.game/dev/doc/code/advanced-topics/hotloading.md)). Game code here is Luau, not C#.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Keep Phase 24's rerun from scratch | Simple | State lost on every save; duplicated parts |
| Swap function bodies in place (s&box's "fast hotload") | Threads keep running mid-wait | Luau can't change a closure's code; it needs VM internals and `unsafe` beyond the debugger's |
| Run the new code but carry top-level locals over by name | Safe Lua only; s&box's field-copy model; handlers rebuilt cleanly | Waiting threads restart (with the kept values); locals the reader can't wrap aren't kept |
| Also keep engine hot-patching, started automatically | One reload story for everything | Paul: game files only; `subsecond` and `dioxus-*` stay out of the build |

## Decision

Carry state over by name. In the editor's Play only, `blocks::hot_reload::wrap` rewrites each top-level `local` on its own line so a reload can read its value (`__axcenity_watch`) and the new run can take it instead of running the initializer (`__axcenity_kept`). Keys are names (`name#2` for a shadowing one), and an initializer whose text changed takes the new value (on purpose unlike s&box, where changed defaults wait for a restart). Functions, threads, connections and `nil` never carry over. `-- @reset` opts a local out. A ModuleScript that loaded as a table keeps that table: the new contents are copied into it. Engine hot-patching is removed; engine changes rebuild and relaunch behind the `engine-dev` feature.

## Consequences

- `script` gains no parser dependency: the editor hands `ScriptRuntime::enable_hot_reload` the wrapper, and the player never wraps, so exported games run scripts exactly as written.
- Lines don't move, so errors, breakpoints and Output links are unchanged.
- A script the reader can't read reloads from scratch, as before.
- Threads waiting in the old run (a `while` loop with `task.wait`) start again in the new run.

## Implementation references

`crates/blocks/src/hot_reload.rs` (wrap), `crates/script/src/hot_reload.rs` (snapshot, keep, merge), `Context::reload_module_keeping`, `ScriptRuntime::reload_script`; tests `crates/blocks/src/hot_reload_tests.rs`.
