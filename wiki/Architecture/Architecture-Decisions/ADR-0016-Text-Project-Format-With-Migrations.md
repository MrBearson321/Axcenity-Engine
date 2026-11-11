---
title: "ADR-0016: Text project files, deterministic, versioned with backed-up migrations"
type: adr
adr: 16
adr_status: Accepted
date: 2026-10-17
affects: [project, anim, save, editor]
source: [crates/project/src/lib.rs, crates/project/src/world_file.rs, crates/project/src/migrate.rs, crates/save/src/migrate.rs]
docs: [docs/project/world.md, docs/project/layout.md, plans/axcenity-engine-v2.md, plans/axcenity-engine-unified.md]
tags: [adr, file-formats]
---

# ADR-0016: Text project files, deterministic, versioned with backed-up migrations

- **Status:** Accepted (v1 plan, 2026-10-17; migration rules v2 plan 2026-10-20 and unified plan)
- **Affected systems:** project format, saves, editor version history

## Context and problem

Projects should diff cleanly in git, be readable and editable by other tools, survive one damaged file, and upgrade safely as the engine changes.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Binary scene files | Fast to load | Opaque diffs and merges |
| RON | Rust-friendly, comments | Few editors highlight or validate it; non-Rust tools can't read it |
| **JSON (deterministic)** | Every editor highlights and validates; JSON Schema possible; any tool reads it | No comments; verbose; slow for huge worlds (S10k's 10 MB `world.json` takes ~5.5 s) |

## Decision

A project is a folder of text files: JSON for data, `.lua` for scripts, `.css` for styles. Serialization is deterministic. `axcenity.json` carries `format_version`: a newer version refuses to open; an older one is upgraded in place after copying the previous files to `.axcenity/previous/<version>/`. **A new optional file doesn't bump the version; a changed shape of an existing file does, with a migration.** One damaged file is a reported problem, not a failed load. Save slots follow the same rule with `SAVE_VERSION` and fixtures.

## Consequences

- Format 5 today (PR #129, release system Phase 7: the manifest records the engine it was created and saved with; the editor asks before upgrading and restores the project if an upgrade fails). The 2 → 3 step rewrote animation files (one clip format, `*.statemachine.json`).
- Loading very large worlds is slow; a binary cache would be a separate decision.

## Implementation references

`crates/project/src/migrate.rs`, `world_file.rs`; `crates/save/src/migrate.rs`; [Project and serialization](../../Engine-Systems/Project-and-Serialization.md), [Change a file format](../../Guides/Recipes/Change-a-File-Format.md).
