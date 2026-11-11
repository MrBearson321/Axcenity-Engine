---
title: Change a file format
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [crates/project/src/lib.rs, crates/project/src/migrate.rs, crates/save/src/migrate.rs, crates/anim/src/files.rs, crates/net/src/lib.rs, crates/assets/src/model/cooked_model.rs]
docs: [docs/project/layout.md, docs/project/world.md, docs/architecture/saves.md, crates/project/CLAUDE.md, crates/save/CLAUDE.md]
tags: [guide, recipe, file-formats]
---

# Change a file format

> [!summary] When to use this
> Changing what's saved in a project, a save slot, a clip, a cooked file, or the network wire.

## Decide which version moves

| You change | Version | Migration |
|---|---|---|
| A new **optional** project file | None | — |
| The shape of an existing project file | `project::FORMAT_VERSION` (now 5; the editor asks before upgrading and puts the project back if the upgrade fails) | A step in `migrate.rs` that keeps a copy of the previous files in `.axcenity/previous/<version>/` |
| A save slot's shape | `save::SAVE_VERSION` (now 2) | A step in `save/src/migrate.rs` and an old slot under `save/src/fixtures/` that a test loads |
| An animation file's shape | `anim::files::FORMAT_VERSION` (now 2) | Older shapes still read (`ClipFile::from_json` converts) |
| A network message or a class default | `net::PROTOCOL_VERSION` (now 10) | None: mismatched versions are refused with a reason; `Hello`/`Refused` stay JSON |
| A field on `Model`, `Primitive`, `ModelMaterial`, `Rig` | Cooked models re-stamp automatically (`AXCENITY_ASSETS_SOURCE_HASH`) | Encode it in `cooked_model.rs` (won't compile otherwise) |
| `.axtex`, `.axgame` | Their own header versions | Old ones still read (`.axgame` v1 unpacks) |

## Steps

1. Read [ADR-0016](../../Architecture/Architecture-Decisions/ADR-0016-Text-Project-Format-With-Migrations.md) and the topic file for the format.
2. Keep serialization deterministic; don't iterate a hash map into output.
3. Update both reading and writing; keep example projects in saved form (re-save them, review the diff).
4. Add the migration and a test that loads an old file and plays the same.
5. Document the shape in the topic file and [File formats](../../Reference/File-Formats.md).

## Pitfalls

- A newer `format_version` than the engine knows refuses to open: never write a version you don't migrate to.
- World-file numbers with 17 significant digits can come back 1 ulp off through serde_json; keep defaults to 15 digits.
- Enum values in `world.json` are written `"Enum.Item"`; a bad one fails the load, and `every_example_renders` doesn't notice (check `project.problems`).
