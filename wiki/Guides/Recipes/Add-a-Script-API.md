---
title: Add a script API
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [crates/core/src/class.rs, crates/script/src/instance.rs, crates/script/src/sides.rs, crates/script/src/definitions.rs, definitions/axcenity.d.luau, axcenity.yml]
docs: [docs/scripting/language.md, docs/scripting/blocks.md, crates/script/CLAUDE.md]
tags: [guide, recipe, scripting]
---

# Add a script API

> [!summary] When to use this
> Adding a class, property, event, method, service or global that Luau scripts can use.

## Before you start

- [Scripting](../../Engine-Systems/Scripting.md), [ADR-0004](../../Architecture/Architecture-Decisions/ADR-0004-Luau-As-The-Only-Script-Runtime.md); `docs/scripting.md` for the topic file of the area.
- Does a Roblox-familiar name exist? Use it (never Roblox branding). Is there already a service that should own it?

## Steps

1. **Declare data in `core`**: a new class or property in `crates/core/src/class.rs` (type, default, enum). Defaults drive serialization and the network.
2. **Bind it in `script`**: property access in `instance.rs` (or the service's module), methods in the class's method table, events with a host firing path. Release World borrows before re-entering Lua; route resumes through `scheduler::resume_as`.
3. **Sides**: if it only works on one side, add it to `SIDE_ONLY_APIS` in `sides.rs` with a helpful message.
4. **Describe it**: add descriptions in `crates/script/src/definitions/docs.rs` (tests fail on an undescribed property, event or global), types in `crates/script/src/definitions/tables.rs`, then run `cargo run -p axcenity-script --bin lua-definitions` to regenerate `definitions/axcenity.d.luau`. A new global also goes in `axcenity.yml` (selene).
5. **A matching block**: every creator-facing API needs a block that translates to it (`crates/blocks-runtime`), plus a row in `blocks/src/tests.rs`'s exhaustive match and in `docs/scripting/blocks.md`.
6. **Project files**: if it's a saved property, update `project` parsing and writing; re-save `examples/primitives/world.json` when the expected layout changes.
7. **Network**: a changed class default raises `net::PROTOCOL_VERSION`.
8. **Docs**: the topic file under `docs/scripting/`; the wiki page if it changes a system's architecture.

## Check your work

| Command | Expect |
|---|---|
| `cargo nextest run -p axcenity-script -p axcenity-blocks -p axcenity-core -p axcenity-project` | Pass, including `definitions_file_is_up_to_date` |
| `luau-lsp analyze --platform=standard --definitions:@axcenity=definitions/axcenity.d.luau <scripts>` | No new warnings |
| An example or game test using it | Behaves the same in editor Play and the player |

## Pitfalls

- A new service changes service lists in several tests (core `new_world_has_game_and_its_services`, Explorer row offsets, GetService errors).
- Adding a Part property breaks `primitives_example_is_already_in_saved_form` and `written_world_has_the_hand_written_layout` until the example is re-saved.
- Never depend on `render` or `audio` from `script`.
