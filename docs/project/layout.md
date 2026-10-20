# Project folder

What a project folder contains, and how a damaged file is handled.

- Readable text files only (except user assets): a manifest, world and Instance data, `.lua` scripts (`*.server.lua` / `*.client.lua` / `*.module.lua`), block script files, `.css` stylesheets, and assets (glTF and FBX models, textures, sounds).
- One damaged file must not prevent loading the rest of the project. Report it and continue. Since Phase 13, `project::load` only fails for a missing or unreadable manifest (not a project folder) or a newer `format_version`. Anything else goes in `Project::problems` and is left out: a broken manifest gives the project its folder's name, a broken `world.json` an empty Workspace (`world_loaded = false`), and an unreadable script (not UTF-8, say) no Script. The player prints problems as `error:` lines and runs the rest.
- The editor watches the folder and reloads files changed externally (see `docs/editor/scripts.md`). Before saving over a world file it never loaded, it renames that file to `world.backup.json` (`project::keep_world_file`).
