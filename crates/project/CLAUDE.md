# project

Loads project folders (`axcenity.json` manifest plus `world.json`) into a `core::World`. Format details: `docs/project-and-export.md`.

- `lib.rs`: `load(folder)` and `LoadError`. Each error names the file; `#[source]` carries the cause, so don't repeat the cause in the message. Only a missing manifest or a newer format fails `load`; other damaged files go in `Project::problems` and are left out (`world_loaded` false = empty Workspace).
- `world_file.rs`: `parse_world(&str)` and `write_world(&World)`. Property JSON is converted using the type of the property's *default* value, so the file stays plain (`[1, 2, 3]`, not `{"Vector3": …}`). The writer is hand-written (serde's pretty printer puts every array item on its own line) and writes every property, sorted, so output is deterministic.
- `lib.rs` also finds `scripts/**/*.server.lua` (sorted by path), and makes a Script for each one in `ServerScriptService`, named after the file. `ScriptFile.path` uses `/` on every OS. `*.client.lua` is ignored until Phase 14.
- `read_script(folder, path)` reads a script file again by its `ScriptFile.path`. Script file operations: `create_script` (`create_new`, never replaces), `write_script`, `rename_script`, `delete_script`, `check_script_name` (Windows rules everywhere), `find_scripts`, `add_script_instance`. `keep_world_file` renames a world file to `world.backup.json` before the editor saves over one it never loaded.
- The world file rejects `Script` entries, and `write_world` skips Script Instances, since scripts are saved as files.
- `save_world` writes `world.json.saving`, then renames it over `world.json`.
- `examples/primitives/world.json` must stay exactly as `write_world` produces it (a test checks this).
- Every serde struct uses `deny_unknown_fields`, so typos become errors with a line number.
- Bump `FORMAT_VERSION` only with a migration path for older files.
- Tests load every folder in `examples/`; keep them all valid.
