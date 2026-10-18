# `project`

- Loads and saves project folders, including CSS source files; see `docs/project-and-export.md` for the file format and compatibility rules.
- Keep serialization deterministic and report damaged files through the existing `Project::problems`/error paths. Bump `FORMAT_VERSION` only with a migration plan.
- When core properties or enums change, update both project parsing and writing, and keep the example-project tests passing.
