# Project format, export and licensing

Read before working in the `project` or `player` crates, the export pipeline, CI, or license files.

## Project folder

- Readable text files only (except user assets): a manifest, world and Instance data, `.lua` scripts (`*.server.lua` / `*.client.lua` / `*.module.lua`), block script files, `.css` stylesheets, and assets (glTF and FBX models, textures, sounds).
- One damaged file must not prevent loading the rest of the project. Report it and continue. Since Phase 13, `project::load` only fails for a missing or unreadable manifest (not a project folder) or a newer `format_version`. Anything else goes in `Project::problems` and is left out: a broken manifest gives the project its folder's name, a broken `world.json` an empty Workspace (`world_loaded = false`), and an unreadable script (not UTF-8, say) no Script. The player prints problems as `error:` lines and runs the rest.
- The editor watches the folder and reloads files changed externally (see `docs/architecture.md`, Script editing). Before saving over a world file it never loaded, it renames that file to `world.backup.json` (`project::keep_world_file`).

### World data format: JSON (decided in Phase 2)

Chosen over RON because every editor (including VS Code) highlights and validates JSON with no extensions, a JSON Schema can later give autocomplete in world files, and non-Rust tools can read and write projects. The cost: JSON has no comments and is a little more verbose.

```text
my-game/
  axcenity.json   { "name": "My Game", "format_version": 1 }
  world.json      the Workspace and its descendants
  scripts/        server scripts: build.server.lua, doors/open.server.lua, ...
                  client scripts: keys.client.lua, ...
                  modules: shared/utils.module.lua, ...
  assets/models/  imported glTF and FBX models: chair.glb, robot.fbx, or
                  tree/tree.gltf with the .bin and textures it uses (see Assets)
  .luarc.json, .axcenity/, .vscode/extensions.json
                  optional: written by the editor's File > Set Up VS Code
                  Autocomplete (see docs/scripting.md); the engine ignores them
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
  },
  "replicated_storage": {
    "children": [ { "class": "RemoteEvent", "name": "MoveRequest" } ]
  }
}
```

- **ReplicatedStorage** (decided in Phase 14, chosen by the developer): the optional `replicated_storage` section is written like `workspace`, and only when it has something in it, so older files and their saved text stay the same. `format_version` stays 1: older engines would reject the new field, but no older engine is in use. These two services are the only saved ones; Players (made at Play) and scripts are never in the file. A `LocalScript` or `Player` listed in the file is an error.

- Property values are plain JSON, and the property's type decides how they're read: `Vector3` and `Color3` are `[a, b, c]`, numbers are numbers, and enum items are strings (`"Shape": "Ball"`, `"Material": "Neon"`). `Color3` is sRGB from 0 to 1.
- Left-out properties keep their defaults. Note `Anchored` defaults to `false`, so a floor or wall left without `"Anchored": true` falls. `name` defaults to the class name. Unknown fields, classes or properties are errors, not silently ignored.
- **Models and Folders** (Phase 11) are written like any other Instance, with `class`, `name` and `children`. They have no properties yet, so the saver leaves out `properties` for them. Parts inside them are drawn and simulated like parts directly in the Workspace.
- `Material` is applied before the other properties wherever it appears in the file, because choosing a material resets `Metalness` and `Roughness` to its preset.
- Saving (`project::save_world`) writes every property, sorted by name, with numbers in their shortest exact form. Loading and saving again always gives the same text. Writing every property, rather than only the changed ones, means a saved world keeps its look even if a default changes later. `examples/primitives` shows the saved layout.
- **Scripts** (decided in Phase 4): every `scripts/**/*.server.lua` becomes a `Script` in `ServerScriptService`, named after the file without `.server.lua`, and scripts start in path order. Subfolders only organize files for now; all Scripts sit directly in `ServerScriptService`. (The Folder class exists since Phase 11, but script subfolders don't turn into Folders yet.) Scripts never appear in `world.json` (loading one there is an error, and saving leaves them out). Putting a script inside a part can be added later as a world-file reference without breaking this layout.
- **Client scripts** (Phase 14): every `scripts/**/*.client.lua` becomes a `LocalScript` in `StarterPlayer.StarterPlayerScripts`, named the same way. `project.scripts` lists Scripts first, then LocalScripts, each by path (`script_order`), which is the order they start in. `script_class(path)`, `script_suffix(class)`, `script_service(world, class)` and `script_path(name, class)` pick the right ending and service.
- **Modules** (Phase 15): every `scripts/**/*.module.lua` becomes a `ModuleScript` in `ReplicatedStorage`, named the same way, after the world file's children there. `script_order` puts modules first (they must be registered before any script requires them), then Scripts, then LocalScripts. Like other scripts, they're never written to `world.json`, and listing one there is an error.
- **Script files** (Phase 13): `project::create_script` (never replaces a file, not even one differing only in capitals on Windows and macOS), `write_script` (through a `.saving` temporary file, like `save_world`), `rename_script` (same folder, refuses to replace another file) and `delete_script`. `check_script_name` applies Windows file name rules on every OS, so projects open everywhere.
- **MeshParts** (Phase 17) are saved like Parts, with `Anchored`, `MeshId`, `Orientation`, `Position` and `Size`. `MeshId` is the model's path from the project folder with `/` between folders (`"MeshId": "assets/models/pyramid.gltf"`); empty means no model. After `#` it can name objects in the file, with `,` between (Phase 18): `"assets/models/props.fbx#Street/Bench,Lamp"` draws only those objects' own meshes (not the objects inside them), and collides as their hull. An object's path is its name and its parents' names from the top, with `/` between; a repeated name among siblings gets `[2]`, `[3]`… on the second and later, and `%`, `/`, `,`, `#`, `[` and `]` in a name are written `%XX` (`assets::Pick`, `model/objects.rs`). Naming an object the file doesn't have is a load error ("it has no object `…`"). Folder and file names in a `MeshId` therefore can't contain `#`; imported copies never do. `examples/hello-mesh` shows the layout. `format_version` stays 1: the new class only adds to the format.
- `format_version` appears in both files. The engine refuses newer versions with a "please update" message.
- Errors name the file and, for JSON syntax problems, the line and column. For bad property values they name the Instance path (for example `Workspace.Block.Color`).

## Assets (decided in Phase 17)

- The `assets` crate is the asset system: it reads every asset file, only from inside the project folder. A `MeshId`, or a URI inside a model, that is absolute, has a drive or scheme (`C:`, `file:`) or a `..` part is refused (`assets::safe_path`), so a world file or script can't read anything else. `data:` URIs embedded in a model are fine.
- `Assets` is a cheap shared handle (one per open project) that loads each model the first time it's asked for, keeps it, and remembers failures so each is reported once (`take_errors`: the editor shows them in Output, the player prints `error:` lines). The project still loads and runs without the model.
- A `MeshId`'s extension says how it's read: `.gltf` and `.glb` are glTF 2.0, `.fbx` is FBX (binary or ASCII, any version). Anything else is refused ("MeshId must name a model file: …").
- `assets::load_model` never crashes on a bad file: errors name the problem ("it isn't a valid glTF file: …", "it isn't a valid FBX file: …", "it uses `x.bin`, which isn't there", "a mesh in it lists a corner that isn't there"), and a panic inside the `gltf` or `ufbx` crate is caught and reported as "the file is damaged".
- FBX textures are looked for inside the model's folder only: at the path the file gives relative to the model, then by file name next to the model, then in a `textures` folder next to it (FBX files often name a texture by where it was on another machine). Embedded textures are used as they are. PNG, JPEG, TGA and BMP decode; a texture that's missing or in another format (`.psd`, `.dds`) doesn't stop the model loading: it's reported once ("The model `…` is drawn without its texture `wood.psd`: …", `Model::warnings`) and the material's color is used.
- Model files aren't checked for changes while the editor runs; they're read again when the project is opened.

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

## Branding (added in Phase 16)

- `assets/branding/` holds the engine's logo, supplied by the developer. See its `README.md` for which file is for what.
- The editor and the player use `icon-256.png` as their window icon, built in with `include_bytes!` (the player decodes it with the `png` crate, the editor with eframe). Windows and X11 show window icons; macOS and Wayland ignore them, so the app bundle and desktop file icons come with export (`icon.ico` and `icon.png` are ready for it).
- The wordmark (added after Phase 16) comes in a light-background and a dark-background version, both made from `wordmark-source.png` by `assets/branding/build.py`. The editor shows it on its start screen and About box; it's ready for splash and loading screens. The window icon is now the hexagon mark on a white rounded square.
- Fonts (`assets/fonts/`, SIL Open Font License 1.1, compatible with Apache-2.0 for bundling) and icons (`egui-phosphor`, MIT) are credited in the editor's About box and must go in the generated credits once the editor is shipped.
- glTF loading (Phase 17) uses `gltf` and `gltf-json` (MIT OR Apache-2.0) and, for textures, `image` (MIT OR Apache-2.0) with its PNG, JPEG, TGA and BMP decoders. FBX loading uses `ufbx` (the Rust bindings and the bundled C library, MIT OR Unlicense; we take it under MIT). All of them go in the generated credits.
