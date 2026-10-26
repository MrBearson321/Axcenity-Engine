# World file

How `world.json` and the other saved Instance files are written.

## Why JSON

Chosen over RON because every editor (including VS Code) highlights and validates JSON with no extensions, a JSON Schema can later give autocomplete in world files, and non-Rust tools can read and write projects. The cost: JSON has no comments and is a little more verbose.

## Folder layout

```text
my-game/
  axcenity.json   { "name": "My Game", "format_version": 3 }
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
                  one folder per animated thing: its clips and state machine
                  (hero/Walk.anim.json, hero/hero.statemachine.json), and
                  Shared/ for clips any model plays; see Animation files
  assets/images/, assets/textures/, assets/particles/
                  UI images, SurfaceAppearance textures and particle textures
  .axcenity/, .vscode/settings.json, .vscode/extensions.json
                  optional: written by the editor's File > Set Up VS Code
                  Autocomplete (see docs/scripting/language.md); the engine ignores them
```

## `world.json`

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

## Properties

- Property values are plain JSON, and the property's type decides how they're read: `Vector3` and `Color3` are `[a, b, c]`, `Vector2` is `[x, y]`, `UDim2` is `[xScale, xOffset, yScale, yOffset]`, numbers are numbers, and enum items are strings (`"Shape": "Ball"`, `"Material": "Neon"`). `Color3` is sRGB from 0 to 1.
- Left-out properties keep their defaults. Note `Anchored` defaults to `false`, so a floor or wall left without `"Anchored": true` falls. `name` defaults to the class name. Unknown fields, classes or properties are errors, not silently ignored.
- **Models and Folders** (Phase 11) are written like any other Instance, with `class`, `name` and `children`. They have no properties yet, so the saver leaves out `properties` for them. Parts inside them are drawn and simulated like parts directly in the Workspace.
- `Material` is applied before the other properties wherever it appears in the file, because choosing a material resets `Metalness` and `Roughness` to its preset.
- Saving (`project::save_world`) writes every property, sorted by name, with numbers in their shortest exact form. Loading and saving again always gives the same text. Writing every property, rather than only the changed ones, means a saved world keeps its look even if a default changes later. `examples/primitives` shows the saved layout.
## Scripts

- **Scripts** (decided in Phase 4): every `scripts/**/*.server.lua` becomes a `Script` in `ServerScriptService`, named after the file without `.server.lua`, and scripts start in path order. Subfolders only organize files for now; all Scripts sit directly in `ServerScriptService`. (The Folder class exists since Phase 11, but script subfolders don't turn into Folders yet.) Scripts never appear in `world.json` (loading one there is an error, and saving leaves them out). Putting a script inside a part can be added later as a world-file reference without breaking this layout.
- **Client scripts** (Phase 14): every `scripts/**/*.client.lua` becomes a `LocalScript` in `StarterPlayer.StarterPlayerScripts`, named the same way. `project.scripts` lists Scripts first, then LocalScripts, each by path (`script_order`), which is the order they start in. `script_class(path)`, `script_suffix(class)`, `script_service(world, class)` and `script_path(name, class)` pick the right ending and service.
- **Modules** (Phase 15): every `scripts/**/*.module.lua` becomes a `ModuleScript` in `ReplicatedStorage`, named the same way, after the world file's children there. `script_order` puts modules first (they must be registered before any script requires them), then Scripts, then LocalScripts. Like other scripts, they're never written to `world.json`, and listing one there is an error.
- **Script files** (Phase 13): `project::create_script` (never replaces a file, not even one differing only in capitals on Windows and macOS), `write_script` (through a `.saving` temporary file, like `save_world`), `rename_script` (same folder, refuses to replace another file) and `delete_script`. `check_script_name` applies Windows file name rules on every OS, so projects open everywhere.
- **UI Event Graphs** (unified Phase 11): `ui/<ScreenGui>.uigraph.json` compiles at load to a LocalScript called `EventGraph` under the Workspace's ScreenGui of that name, after the other scripts; its path is the graph's. See `docs/scripting/ui-graphs.md`.
- **Block scripts** (Phase 29): `*.server.blocks.json` files become server Scripts and `*.client.blocks.json` files become LocalScripts. The JSON is `{ "blocks": [...] }`: the canvas's top-level stacks, each a block `{ "id", "kind": { "type": "when_started", "body": [...] , ... }, "position": [x, y] }`. `position` (canvas units) is only written once a stack has been moved; without it, stacks are laid out down the left. A top-level `unattached` block holds loose blocks that don't run. Value slots hold `{ "type": "number", "value": 1 }` and the like, or reporter blocks (`math`, `compare`, `property`, ...). An optional `graph` object holds the Graph view (`shown`, node places in `nodes`, loose value nodes in `loose`); it's only written once the script has been shown as a graph, and never changes the Lua (`docs/scripting/graphs.md`). Condition slots also read the first format's `{ "left", "operator", "right" }` comparison, so older files still load. Project loading translates block scripts to Lua and registers them as ordinary scripts. The editor writes the block data atomically through the same temporary-file saver, and block errors map to their originating blocks. See `docs/scripting/blocks.md` for the blocks and the Lua each becomes.
## Other saved data

- **Sound assets** (Phase 29): `Sound.SoundId` is a relative path such as `"assets/audio/chime.wav"`. Hosts read bytes through `Assets`, which rejects paths outside the project. Kira decodes WAV, FLAC, Ogg Vorbis and MP3 files; audio is loaded lazily when a sound first plays. `Spatial` (default false), `Position`, `RollOffMinDistance` (10) and `RollOffMaxDistance` (150) are ordinary Sound properties: left out, a saved sound stays non-spatial. `examples/hello-spatial` parents two spatial Sounds under parts.
- **MeshParts** (Phase 17) are saved like Parts, with `Anchored`, `MeshId`, `Orientation`, `Position` and `Size`. `MeshId` is the model's path from the project folder with `/` between folders (`"MeshId": "assets/models/pyramid.gltf"`); empty means no model. After `#` it can name objects in the file, with `,` between (Phase 18): `"assets/models/props.fbx#Street/Bench,Lamp"` draws only those objects' own meshes (not the objects inside them), and collides as their hull. An object's path is its name and its parents' names from the top, with `/` between; a repeated name among siblings gets `[2]`, `[3]`… on the second and later, and `%`, `/`, `,`, `#`, `[` and `]` in a name are written `%XX` (`assets::Pick`, `model/objects.rs`). Naming an object the file doesn't have is a load error ("it has no object `…`"). Folder and file names in a `MeshId` therefore can't contain `#`; imported copies never do. `examples/hello-mesh` shows the layout. `format_version` stays 1: the new class only adds to the format.
- **Parts** have `TileSize` (studs one copy of a SurfaceAppearance's textures covers, default 8; 0 leaves the shape's coordinates unscaled). Older worlds leave it out and load the default. `format_version` stays 1.
- **SurfaceAppearances** (Phase 21) are saved inside their Part or MeshPart like any child, with every property: `MaterialName`, `Color`, the maps (`ColorMap`, `NormalMap`, `MetalnessMap`, `RoughnessMap`, `EmissiveMap`, `AmbientOcclusionMap`, `OpacityMap`, `SpecularMap`: texture paths from the project folder, `""` for none), `NormalStrength`, `NormalMapDirectX`, the four `…Channel`s (`"Red"`, `"Green"`, `"Blue"` or `"Alpha"`), `InvertRoughness`, `InvertOpacity`, `AmbientOcclusionStrength` and `EmissiveStrength`. Texture paths follow the same rules as `MeshId` (inside the project only).
- **UI Instances** (Phase 22) are saved in the Workspace like other Instances. `ScreenGui` holds `Frame` and `TextLabel` children; a UDim2 position/size is written as `[xScale, xOffset, yScale, yOffset]`.
- **CSS stylesheets** (Phase 23) are plain `.css` files anywhere in the project folder. The loader reads them in stable path order; a file that cannot be read is reported without blocking the rest of the project.
- **Prefabs** (Phase 34): `assets/prefabs/*.prefab.json` holds Instances saved to be placed again: `{ "format_version": 1, "instances": [ ... ] }`, each written exactly like a child in `world.json` (scripts are left out). `project::parse_prefab` checks the whole file before adding anything, so a bad prefab adds nothing; `project::write_prefab` writes it. Positions are absolute as saved; the editor moves the whole prefab where it's placed.
## Versions and errors

- `format_version` appears in both files. The manifest's number is the project format (`3` in this engine; 3 is the one clip format and state machines, v3 Phase 7). Opening an older manifest upgrades the project and keeps the previous files; see `docs/project/layout.md`. A newer manifest refuses to open and asks the creator to update Axcenity. `world.json` and prefab files stay at format `1` until their own shape changes, and a newer world file still refuses to load.
- Errors name the file and, for JSON syntax problems, the line and column. For bad property values they name the Instance path (for example `Workspace.Block.Color`).
