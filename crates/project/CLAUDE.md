# `project`

- Loads and saves project folders, including CSS source files. Open one file from the map in `docs/project-and-export.md`.
- `lib.rs` is the manifest, scripts and stylesheets. `world_file.rs` is `world.json`. `terrain_files.rs` is Terrains' data folders (codecs in `terrain::files`). `packed.rs` packs a game. `versions.rs` keeps save history.
- Keep serialization deterministic and report damaged files through the existing `Project::problems`/error paths. Bump `FORMAT_VERSION` only with a migration plan.
- When core properties or enums change, update both project parsing and writing, and keep the example-project tests passing.
