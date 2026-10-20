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
                  block scripts: logic.server.blocks.json, ui.client.blocks.json, ...
  ui.css          project stylesheet (additional .css files may be in subfolders)
  assets/models/  imported glTF and FBX models: chair.glb, robot.fbx, or
                  tree/tree.gltf with the .bin and textures it uses (see Assets)
  assets/audio/   project audio files referenced by Sound.SoundId
  assets/prefabs/ prefabs saved from the editor's Assets panel: lamp.prefab.json, ...
  assets/animations/
                  animation clips (hero/Walk.anim.json, ...) and animator
                  controllers (hero.controller.json), see Animation files
  assets/images/, assets/textures/, assets/particles/
                  UI images, SurfaceAppearance textures and particle textures
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

- **Lighting** (Phase 30): the `Lighting` service's properties (`ShadowsEnabled`, `ShadowDistance`) are saved as an optional `lighting` section, `{ "properties": { ... } }`, only when one differs from its default, so older files and their saved text stay the same. Nothing can be saved inside Lighting.
- **ReplicatedStorage** (decided in Phase 14, chosen by the developer): the optional `replicated_storage` section is written like `workspace`, and only when it has something in it, so older files and their saved text stay the same. `format_version` stays 1: older engines would reject the new field, but no older engine is in use. These two services are the only saved ones; Players (made at Play) and scripts are never in the file. A `LocalScript` or `Player` listed in the file is an error.

- Property values are plain JSON, and the property's type decides how they're read: `Vector3` and `Color3` are `[a, b, c]`, `UDim2` is `[xScale, xOffset, yScale, yOffset]`, numbers are numbers, and enum items are strings (`"Shape": "Ball"`, `"Material": "Neon"`). `Color3` is sRGB from 0 to 1.
- Left-out properties keep their defaults. Note `Anchored` defaults to `false`, so a floor or wall left without `"Anchored": true` falls. `name` defaults to the class name. Unknown fields, classes or properties are errors, not silently ignored.
- **Models and Folders** (Phase 11) are written like any other Instance, with `class`, `name` and `children`. They have no properties yet, so the saver leaves out `properties` for them. Parts inside them are drawn and simulated like parts directly in the Workspace.
- `Material` is applied before the other properties wherever it appears in the file, because choosing a material resets `Metalness` and `Roughness` to its preset.
- Saving (`project::save_world`) writes every property, sorted by name, with numbers in their shortest exact form. Loading and saving again always gives the same text. Writing every property, rather than only the changed ones, means a saved world keeps its look even if a default changes later. `examples/primitives` shows the saved layout.
- **Scripts** (decided in Phase 4): every `scripts/**/*.server.lua` becomes a `Script` in `ServerScriptService`, named after the file without `.server.lua`, and scripts start in path order. Subfolders only organize files for now; all Scripts sit directly in `ServerScriptService`. (The Folder class exists since Phase 11, but script subfolders don't turn into Folders yet.) Scripts never appear in `world.json` (loading one there is an error, and saving leaves them out). Putting a script inside a part can be added later as a world-file reference without breaking this layout.
- **Client scripts** (Phase 14): every `scripts/**/*.client.lua` becomes a `LocalScript` in `StarterPlayer.StarterPlayerScripts`, named the same way. `project.scripts` lists Scripts first, then LocalScripts, each by path (`script_order`), which is the order they start in. `script_class(path)`, `script_suffix(class)`, `script_service(world, class)` and `script_path(name, class)` pick the right ending and service.
- **Modules** (Phase 15): every `scripts/**/*.module.lua` becomes a `ModuleScript` in `ReplicatedStorage`, named the same way, after the world file's children there. `script_order` puts modules first (they must be registered before any script requires them), then Scripts, then LocalScripts. Like other scripts, they're never written to `world.json`, and listing one there is an error.
- **Script files** (Phase 13): `project::create_script` (never replaces a file, not even one differing only in capitals on Windows and macOS), `write_script` (through a `.saving` temporary file, like `save_world`), `rename_script` (same folder, refuses to replace another file) and `delete_script`. `check_script_name` applies Windows file name rules on every OS, so projects open everywhere.
- **Block scripts** (Phase 29): `*.server.blocks.json` files become server Scripts and `*.client.blocks.json` files become LocalScripts. The JSON is `{ "blocks": [...] }`: the canvas's top-level stacks, each a block `{ "id", "kind": { "type": "when_started", "body": [...] , ... }, "position": [x, y] }`. `position` (canvas units) is only written once a stack has been moved; without it, stacks are laid out down the left. A top-level `unattached` block holds loose blocks that don't run. Value slots hold `{ "type": "number", "value": 1 }` and the like, or reporter blocks (`math`, `compare`, `property`, ...). Condition slots also read the first format's `{ "left", "operator", "right" }` comparison, so older files still load. Project loading translates block scripts to Lua and registers them as ordinary scripts. The editor writes the block data atomically through the same temporary-file saver, and block errors map to their originating blocks. See `docs/scripting.md` for the blocks and the Lua each becomes.
- **Sound assets** (Phase 29): `Sound.SoundId` is a relative path such as `"assets/audio/chime.wav"`. Hosts read bytes through `Assets`, which rejects paths outside the project. Kira decodes WAV, FLAC, Ogg Vorbis and MP3 files; audio is loaded lazily when a sound first plays.
- **MeshParts** (Phase 17) are saved like Parts, with `Anchored`, `MeshId`, `Orientation`, `Position` and `Size`. `MeshId` is the model's path from the project folder with `/` between folders (`"MeshId": "assets/models/pyramid.gltf"`); empty means no model. After `#` it can name objects in the file, with `,` between (Phase 18): `"assets/models/props.fbx#Street/Bench,Lamp"` draws only those objects' own meshes (not the objects inside them), and collides as their hull. An object's path is its name and its parents' names from the top, with `/` between; a repeated name among siblings gets `[2]`, `[3]`… on the second and later, and `%`, `/`, `,`, `#`, `[` and `]` in a name are written `%XX` (`assets::Pick`, `model/objects.rs`). Naming an object the file doesn't have is a load error ("it has no object `…`"). Folder and file names in a `MeshId` therefore can't contain `#`; imported copies never do. `examples/hello-mesh` shows the layout. `format_version` stays 1: the new class only adds to the format.
- **SurfaceAppearances** (Phase 21) are saved inside their MeshPart like any child, with every property: `MaterialName`, `Color`, the maps (`ColorMap`, `NormalMap`, `MetalnessMap`, `RoughnessMap`, `EmissiveMap`, `AmbientOcclusionMap`, `OpacityMap`, `SpecularMap`: texture paths from the project folder, `""` for none), `NormalStrength`, `NormalMapDirectX`, the four `…Channel`s (`"Red"`, `"Green"`, `"Blue"` or `"Alpha"`), `InvertRoughness`, `InvertOpacity`, `AmbientOcclusionStrength` and `EmissiveStrength`. Texture paths follow the same rules as `MeshId` (inside the project only).
- **UI Instances** (Phase 22) are saved in the Workspace like other Instances. `ScreenGui` holds `Frame` and `TextLabel` children; a UDim2 position/size is written as `[xScale, xOffset, yScale, yOffset]`.
- **CSS stylesheets** (Phase 23) are plain `.css` files anywhere in the project folder. The loader reads them in stable path order; a file that cannot be read is reported without blocking the rest of the project.
- **Prefabs** (Phase 34): `assets/prefabs/*.prefab.json` holds Instances saved to be placed again: `{ "format_version": 1, "instances": [ ... ] }`, each written exactly like a child in `world.json` (scripts are left out). `project::parse_prefab` checks the whole file before adding anything, so a bad prefab adds nothing; `project::write_prefab` writes it. Positions are absolute as saved; the editor moves the whole prefab where it's placed.
- **Animation files** (Phase 38, `anim::files`): pretty JSON, written with every field in a fixed order (`to_json`, through a `.saving` temporary file), so saving the same thing gives the same text; unknown fields are errors that name the line. Paths in them are from the project folder.
  - A **clip** (`*.anim.json`, named by its file): `{ "format_version": 1, "source": "assets/models/noodle.fbx", "take": "Moves", "start": 0.0, "end": 1.0, "speed": 1.0, "loop": true, "root_motion": false, "events": [ { "time": 0.5, "name": "Wave" } ] }`. `start` and `end` are seconds in the take; `speed`, `loop`, `root_motion` and `events` may be left out (1, true, false, none). An event's `time` is seconds from the clip's start. The Animation panel writes them (see `docs/architecture.md`).
  - A **controller** (`*.controller.json`): `{ "format_version": 1, "parameters": [ { "type": "Bool", "name": "IsGrounded", "default": true }, { "type": "Number", "name": "Speed", "default": 0.0 }, { "type": "Trigger", "name": "Jump" } ], "default_state": "Idle", "states": [ { "name": "Idle", "motion": { "clip": "assets/animations/noodle/Idle.anim.json" }, "speed": 1.0, "position": [0.0, 0.0] } ], "transitions": [ { "from": "Any State", "to": "Jump", "conditions": [ { "parameter": "Jump", "test": "Triggered", "value": 0.0 } ], "blend": 0.25, "exit_time": null, "interruptible": false, "priority": 0 } ] }`. `position` places a state in the editor's graph. Condition tests are `IsTrue`, `IsFalse` (Bools), `Triggered` (Triggers), `Greater` and `Less` (Numbers, against `value`). An empty or unknown `default_state` starts in the first state. Phase 38 plays states, the default state and `Play`; parameters and transitions are read and kept, and run from Phase 40.
  - `examples/hello-animation` has a generated rig (`noodle.fbx`, from `assets` `model/test_rig.rs`; a test checks it's up to date, and `AXCENITY_WRITE_EXAMPLES=1` rewrites it), four clips and a controller; tests check those files are saved as the engine writes them.
- `format_version` appears in both files. The engine refuses newer versions with a "please update" message.
- Errors name the file and, for JSON syntax problems, the line and column. For bad property values they name the Instance path (for example `Workspace.Block.Color`).

## Assets (decided in Phase 17)

- The `assets` crate is the asset system: it reads every asset file, only from inside the project folder. A `MeshId`, or a URI inside a model, that is absolute, has a drive or scheme (`C:`, `file:`) or a `..` part is refused (`assets::safe_path`), so a world file or script can't read anything else. `data:` URIs embedded in a model are fine.
- `Assets` is a cheap shared handle (one per open project) that loads each model the first time it's asked for, keeps it, and remembers failures so each is reported once (`take_errors`: the editor shows them in Output, the player prints `error:` lines). The project still loads and runs without the model.
- A `MeshId`'s extension says how it's read: `.gltf` and `.glb` are glTF 2.0, `.fbx` is FBX (binary or ASCII, any version). Anything else is refused ("MeshId must name a model file: …").
- `assets::load_model` never crashes on a bad file: errors name the problem ("it isn't a valid glTF file: …", "it isn't a valid FBX file: …", "it uses `x.bin`, which isn't there", "a mesh in it lists a corner that isn't there"), and a panic inside the `gltf` or `ufbx` crate is caught and reported as "the file is damaged".
- FBX textures are looked for inside the model's folder only: at the path the file gives relative to the model, then by file name next to the model, then in a `textures` folder next to it (FBX files often name a texture by where it was on another machine). Embedded textures are used as they are, and importing copies each out (its bytes as they are) to the model's folder as `textures/<its file name>` (`embedded1.png`… if it has none; a number is added if taken), so a model with embedded textures is always imported into a folder of its own (Phase 20). A glTF's embedded images (in a `.glb`, or `data:` URIs) are copied out the same way since Phase 21, named after the image (or `image<N>`), so every map a model uses is a file a SurfaceAppearance can name; a loaded `Image` remembers its file (`Image::file`, relative to the model's folder). PNG, JPEG, TGA and BMP decode; a texture that's missing or in another format (`.psd`, `.dds`) doesn't stop the model loading: it's reported once ("The model `…` is drawn without its texture `wood.psd`: …", `Model::warnings`) and the material's color is used.
- Model files aren't checked for changes while the editor runs; they're read again when the project is opened.

## Export

- File > Export Game translates block scripts to Lua and writes `game.axgame`, a small versioned archive containing the manifest, world, scripts, CSS and assets. Editor-only files are excluded. The player unpacks it to a private temporary folder and runs it; the archive refuses traversal and duplicate paths.
- The editor pairs the archive with a player executable. The current-OS player is looked up beside the editor. Cross-target players are looked up under `players/windows`, `players/macos` or `players/linux` beside it; distributions must supply those binaries. Without the requested player, export reports an error rather than silently falling back to another OS.
- Every export writes `LICENSES.txt` with the engine's `NOTICE`. macOS exports are unsigned `.app` bundles in v1.
- **Version history**: each successful save copies `world.json`, `scripts/` and every `.css` file into `.axcenity/history/<id>/` (`project::record_version`). A second save with the same files is skipped. The newest 40 are kept. File > Version History in the editor, and Version History on a project card on the start screen, list them and can restore one (`project::restore_version`): those files are copied back, and scripts or styles the version doesn't have are removed. Assets, `axcenity.json` and `thumbnail.png` are left as they are. The current files are recorded first (labeled "Before restore" when they weren't already the newest version). `.axcenity` is not exported.
- The "Made with Axcenity" splash is on by default. File > Export Game > Show splash in exported games stores `show_splash` in `axcenity.json`; missing values default to true.
- The manifest also takes an optional `description` (default empty). The editor's start screen edits `name`, `description` and `show_splash` from a project's Settings, and New Project uses `project::create_project_from` with the chosen `Template`: Baseplate (a grey baseplate), Empty, or Starter scene (a lit scene with a spinning neon part, a touch pad, and the block script `scripts/Starter.server.blocks.json` that drives them). `thumbnail.png` (320×180) in the project folder is the editor's project-list picture, written on Save and Close Project from a viewport screenshot; it isn't exported.

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
