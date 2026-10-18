# Project format, export and licensing

Read before working in the `project` or `player` crates, the export pipeline, CI, or license files.

## Project folder

- Readable text files only (except user assets): a manifest, world and Instance data, `.lua` scripts (`*.server.lua` / `*.client.lua`), block script files, `.css` stylesheets, and assets (glTF, textures, sounds).
- The world data format (RON or JSON) is still open. Decide it in the `project` crate and record it here.
- One damaged file must not prevent loading the rest of the project. Report it and continue.
- The editor watches the folder and reloads files changed externally.

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
