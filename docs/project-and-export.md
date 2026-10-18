# Project format, export and licensing

Read before working in the `project` or `player` crates, the export pipeline, CI, or license files.

## Project folder

- Readable text files only (except user assets): a manifest, world and Instance data, `.lua` scripts (`*.server.lua` / `*.client.lua`), block script files, `.css` stylesheets, and assets (glTF, textures, sounds).
- One damaged file must not prevent loading the rest of the project. Report it and continue.
- The editor watches the folder and reloads files changed externally.

### World data format: JSON (decided in Phase 2)

Chosen over RON because every editor (including VS Code) highlights and validates JSON with no extensions, a JSON Schema can later give autocomplete in world files, and non-Rust tools can read and write projects. The cost: JSON has no comments and is a little more verbose.

```text
my-game/
  axcenity.json   { "name": "My Game", "format_version": 1 }
  world.json      the Workspace and its descendants
```

`world.json`:

```json
{
  "format_version": 1,
  "workspace": {
    "children": [
      { "class": "Part", "name": "Block",
        "properties": { "Position": [0, 1, 0], "Color": [0.8, 0.2, 0.2] },
        "children": [] }
    ]
  }
}
```

- Property values are plain JSON, and the property's type decides how they're read: `Vector3` and `Color3` are `[a, b, c]`, numbers are numbers, and enum items are strings (`"Shape": "Ball"`, `"Material": "Neon"`). `Color3` is sRGB from 0 to 1.
- Left-out properties keep their defaults. `name` defaults to the class name. Unknown fields, classes or properties are errors, not silently ignored.
- `Material` is applied before the other properties wherever it appears in the file, because choosing a material resets `Metalness` and `Roughness` to its preset.
- Saving (`project::save_world`) writes every property, sorted by name, with numbers in their shortest exact form. Loading and saving again always gives the same text. Writing every property, rather than only the changed ones, means a saved world keeps its look even if a default changes later. `examples/primitives` shows the saved layout.
- `format_version` appears in both files. The engine refuses newer versions with a "please update" message.
- Errors name the file and, for JSON syntax problems, the line and column. For bad property values they name the Instance path (for example `Workspace.Block.Color`).

## Export

- CI (GitHub Actions) builds `player` for Windows, macOS and Linux. The editor ships with those pre-built players.
- Export (run by the editor) translates blocks to Lua via `blocks`, then `project` packs the project into **one game file**, and pairs it with each chosen OS's player. Exporting for any OS works from any OS.
- Export always writes a licenses and credits file that includes the engine's `NOTICE`.
- The "Made with Axcenity" splash is on by default, with a per-project toggle to turn it off.
- macOS builds are unsigned in v1.

## Licensing

- The engine is **Apache-2.0**. The `NOTICE` reads: "Built with Axcenity Engine, © 2026 Paul Janota and contributors."
- Every new dependency must be license-compatible with Apache-2.0 (MIT, Apache-2.0, BSD, Zlib and similar are fine; no GPL). Add its notice to the generated credits.
- Games made with the engine belong to their creators, with no royalties.
