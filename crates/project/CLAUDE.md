# project

Loads project folders (`axcenity.json` manifest plus `world.json`) into a `core::World`. Format details: `docs/project-and-export.md`.

- `lib.rs`: `load(folder)` and `LoadError`. Each error names the file; `#[source]` carries the cause, so don't repeat the cause in the message.
- `world_file.rs`: `parse_world(&str)`. Property JSON is converted using the type of the property's *default* value, so the file stays plain (`[1, 2, 3]`, not `{"Vector3": …}`).
- Every serde struct uses `deny_unknown_fields`, so typos become errors with a line number.
- Bump `FORMAT_VERSION` only with a migration path for older files.
- Tests load `examples/hello-part`; keep that example valid.
