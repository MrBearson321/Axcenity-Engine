---
title: "ADR-0017: Editor Play runs in-process on a cloned World"
type: adr
adr: 17
adr_status: Accepted
date: 2026-10-18
affects: [editor, sim, script, physics]
source: [crates/editor/src/play.rs, crates/editor/src/play/multi.rs, crates/editor/src/play/kept.rs]
docs: [docs/architecture/play-mode.md]
tags: [adr, editor]
---

# ADR-0017: Editor Play runs in-process on a cloned World

- **Status:** Accepted (v1 Phase 12; extended by unified Phases 24 and 64–67)
- **Affected systems:** editor, Play, the frame schedule

## Context and problem

Pressing Play must start fast, show the live game in the Explorer and Properties, survive script errors and engine panics, and leave the edit world untouched on Stop.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Launch the player as a separate process | Perfect isolation | Slow start; no live Explorer/Properties; harder debugging |
| **Run in-process on `World::clone()`** | Instant; inspect and edit the live world; breakpoints; several clients over loopback | A panic must be contained (`catch_unwind`); Play's GPU drawing shares the editor's device |

## Decision

Play autosaves if needed, deep-copies the World (every ID stays valid), reads scripts from disk, and runs a fresh `ScriptRuntime` and `Physics` through the shared schedule ([ADR-0006](ADR-0006-One-Shared-Frame-Schedule.md)). Stop drops the copy. Properties edits during Play change the game world and are discarded unless **kept**. Panics in `Session::start`/`frame` are caught and reported. Several clients are extra `Session`s joined over the in-memory loopback.

## Consequences

- Panics while *drawing* the Play world aren't caught yet.
- Editor Play can't host or join a real network game.
- Requires `panic = "unwind"` in every profile.

## Implementation references

`crates/editor/src/play.rs`, `play/multi.rs`, `play/kept.rs`; [Editor architecture](../../Editor/Editor-Architecture.md#play-mode).
