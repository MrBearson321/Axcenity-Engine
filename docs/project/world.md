# World file

How `world.json` and the other saved Instance files are written.

## Why JSON

Chosen over RON because every editor (including VS Code) highlights and validates JSON with no extensions, a JSON Schema can later give autocomplete in world files, and non-Rust tools can read and write projects. The cost: JSON has no comments and is a little more verbose.

## Folder layout

```text
my-game/
  axcenity.json   { "name": "My Game", "format_version": 4 }
  world.json      the Workspace and its descendants
  scripts/        server scripts: build.server.lua, doors/open.server.lua, ...
                  client scripts: keys.client.lua, ...
                  modules: shared/utils.module.lua, ...
                  block scripts: logic.server.blocks.json, ui.client.blocks.json, ...
  levels/         optional: the levels other than the start level (world.json),
                  each <name>.world.json with only its workspace (see Levels)
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

- **Lighting** (Phase 30): the `Lighting` service's properties (`ShadowsEnabled`, `ShadowDistance`, `ClockTime`...) are saved as an optional `lighting` section, `{ "properties": { ... } }`, only when one differs from its default or something is inside Lighting, so older files and their saved text stay the same. What's inside Lighting (an `Atmosphere`, graphics Phase 24) is saved as its `children`, like the Workspace's.
- **ReplicatedStorage** (decided in Phase 14, chosen by the developer): the optional `replicated_storage` section is written like `workspace`, and only when it has something in it, so older files and their saved text stay the same. `format_version` stays 1: older engines would reject the new field, but no older engine is in use. Players (made at Play) and scripts are never in the file. A `LocalScript` or `Player` listed in the file is an error.
- **Levels** (Phase 48): `world.json` is the start level; `levels/<name>.world.json` files hold other levels' Workspaces, written by `project::write_level` in this layout with only `workspace`. See [levels.md](levels.md).
- **SoundService** (Phase 52): an optional `sound_service` section holds its SoundGroups (with their sound effects) as `children`, written only when it has something in it. A `Sound`'s `SoundGroup` is a reference like an ObjectValue's, a path from `SoundService` (`["SoundService", "Master", "Music"]`).
- **Players, StarterPlayer and Teams** (Phase 47): optional `players` (`CharacterAutoLoads`, `RespawnTime`, `LeaderboardEnabled`), `starter_player` (`StarterCharacter`, a prefab path) and `teams` (its `Team` children) sections, in Lighting's shape and written the same way: only when a setting isn't its default or something is inside. StarterPlayerScripts is never written (it's always there). The examples without a character of their own have `"players": { "properties": { "CharacterAutoLoads": false } }`.

## Properties

- Property values are plain JSON, and the property's type decides how they're read: `Vector3` and `Color3` are `[a, b, c]`, `Vector2` is `[x, y]`, `UDim2` is `[xScale, xOffset, yScale, yOffset]`, numbers are numbers, and enum items are strings (`"Shape": "Ball"`, `"Material": "Neon"`). `Color3` is sRGB from 0 to 1.
- **CFrame and references** (Phase 29): a `CFrame` (`CFrameValue.Value`) is `[x, y, z, qx, qy, qz, qw]`, its position then its rotation as a quaternion (normalized on load; all zeros is an error). An `ObjectValue`'s `Value` is the path of names to its target, `["Workspace", "Door"]`, or `null`. In `world.json` the path starts at `Workspace` or `ReplicatedStorage`; in a prefab it starts at one of the prefab's own top-level Instances, so a placed copy points into itself, and a target outside the prefab is saved as `null`. Paths are looked up once every Instance exists; through same-named siblings they find the first, and one that names nothing loads as `null`.
- **Attributes and tags** (Phase 29): any Instance (not a service) can have `"attributes": { "Damage": 10, "Spawn": { "Vector3": [0, 5, 0] } }` and `"tags": ["Lava", "Hot"]`, written after `properties` and only when not empty, so older files and their saved text stay the same and `format_version` stays 1. Booleans, numbers and strings are plain JSON; other values are an object with one field naming the type: `Vector3`, `Vector2`, `Color3`, `UDim2`, `CFrame` (as above) or `EnumItem` (`"Material.Neon"`). Attributes are sorted by name; tags keep their order. A bad one is an error naming the Instance and attribute (`Workspace.Lava: attribute \`Spawn\`: …`). Prefabs keep them the same way.
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
- **Bodies** (Phase 34): Parts and MeshParts have `Massless`, `AssemblyLinearVelocity`, `AssemblyAngularVelocity` and `CustomPhysicalProperties`, which is `null` (the material's) or `{ "Density": 1, "Friction": 0.5, "Elasticity": 0.25, "FrictionWeight": 1, "ElasticityWeight": 1 }`; numbers left out are Plastic's and each is clamped to its range. The Workspace has `Gravity` (written only when it isn't 196.2). Materials gained `Ice` and `Rubber`. A save slot holds the value as `{"PhysicalProperties": [d, f, e, fw, ew]}`. `format_version` stays 1; the project format became 4 so older Metal parts keep their physics (`docs/project/layout.md`).
- **SurfaceAppearances** (Phase 21) are saved inside their Part or MeshPart like any child, with every property: `MaterialName`, `Color`, the maps (`ColorMap`, `NormalMap`, `MetalnessMap`, `RoughnessMap`, `EmissiveMap`, `AmbientOcclusionMap`, `OpacityMap`, `SpecularMap`: texture paths from the project folder, `""` for none), `NormalStrength`, `NormalMapDirectX`, the four `…Channel`s (`"Red"`, `"Green"`, `"Blue"` or `"Alpha"`), `InvertRoughness`, `InvertOpacity`, `AmbientOcclusionStrength` and `EmissiveStrength`. Texture paths follow the same rules as `MeshId` (inside the project only).
- **UI Instances** (Phase 22) are saved in the Workspace like other Instances. `ScreenGui` holds `Frame` and `TextLabel` children; a UDim2 position/size is written as `[xScale, xOffset, yScale, yOffset]`.
- **CSS stylesheets** (Phase 23) are plain `.css` files anywhere in the project folder. The loader reads them in stable path order; a file that cannot be read is reported without blocking the rest of the project.
- **Prefabs** (Phase 34): `assets/prefabs/*.prefab.json` holds Instances saved to be placed again: `{ "format_version": 1, "instances": [ ... ] }`, each written exactly like a child in `world.json` (scripts are left out). `project::parse_prefab` checks the whole file before adding anything, so a bad prefab adds nothing; `project::write_prefab` writes it. Positions are absolute as saved; the editor moves the whole prefab where it's placed.
- **Linked copies, nesting and variants** (Phase 49; `core::prefab`, `project/src/prefabs.rs`): a placed copy of a prefab is still written in full, in `world.json`, a level, or another prefab, so every file reads on its own. Its top-level Instance also has `"prefab": "assets/prefabs/Car.prefab.json"` (and `"prefab_index": 1` for a prefab's second top-level Instance). Inside the copy, an Instance whose values differ from the prefab lists them in `"overrides": ["Color", "Position"]` (`"attributes"` and `"tags"` mean its attributes or tags are its own), a child the copy added has `"added": true`, and a parent lists the prefab's children the copy took out in `"removed": ["Door", ["Wheel", 3]]`. Children are matched by name and by which of that name they are (`["Wheel", 3]` is the fourth Wheel; a plain name is the first), so a child renamed in a copy is removed and added. A prefab holding copies of other prefabs is **nested**; a **variant** is a prefab whose top-level Instance is a copy of its **base** (`project::variant_text`). Loading (`project::load_with_level`, `read_level`, `read_start_level`, `load_starter_character`) re-makes each copy from its prefab file as it is now and puts its overrides back (`Prefabs::refresh`, then `prefab::sync`), so a change to Wheel reaches the wheels in every Car, in every level, and a variant keeps what it overrides. A prefab that's missing (or holds itself) leaves its copies as saved and is a load problem (`LoadError::Prefab`). In memory only a copy's top-level Instance holds a `PrefabLink` (path, index, the source it was made from); overrides are worked out from that source (`prefab::overrides`). Files without these fields, including every prefab saved before Phase 49, load and save exactly as before.
## Versions and errors

- `format_version` appears in both files. The manifest's number is the project format (`4` in this engine; 3 is the one clip format and state machines, v3 Phase 7; 4 physical properties, Phase 34). Opening an older manifest upgrades the project and keeps the previous files; see `docs/project/layout.md`. A newer manifest refuses to open and asks the creator to update Axcenity. `world.json` and prefab files stay at format `1` until their own shape changes, and a newer world file still refuses to load.
- Errors name the file and, for JSON syntax problems, the line and column. For bad property values they name the Instance path (for example `Workspace.Block.Color`).
