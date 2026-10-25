# `project`

- Loads and saves project folders, including CSS source files. Open one file from the map in `docs/project-and-export.md`.
- `lib.rs` is the manifest, load and save; `scripts.rs`, `stylesheets.rs` and `templates.rs` hold those (re-exported from `lib.rs`). `world_file.rs` is `world.json`. `migrate.rs` upgrades an older manifest and keeps the previous files (`migrate_animation.rs` is the 2 → 3 step: one clip format, state machines). `terrain_files.rs` is Terrains' data folders (codecs in `terrain::files`). `packed.rs` packs a game. `versions.rs` keeps save history. `validate.rs` is the editor's asset check (`check_assets`); it must not write the project.
- Keep serialization deterministic and report damaged files through the existing `Project::problems`/error paths. Bump `FORMAT_VERSION` only with a migration plan.
- When core properties or enums change, update both project parsing and writing, and keep the example-project tests passing.
