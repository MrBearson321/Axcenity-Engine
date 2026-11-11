# Scripts inside objects (G2): design note

Roadmap item **G2** from [engine-gap-analysis.md](engine-gap-analysis.md) (PR #89). Paul approved the storage on 11 Nov 2026: a script inside an object is a reference to a `.lua` file, not code stored in `world.json`. This note is the design to review before any code. Nothing here is built yet.

## Goal

A Model or Part can carry its own Script, LocalScript or ModuleScript, as in Roblox, so a door, a pickup or a Toolbox model works wherever it's placed or copied. `script.Parent` is the object. Code stays in files, so git, the code editor, Find in Scripts and live reload keep working.

## What exists today

- `project/src/scripts.rs` scans `scripts/**/*.server.lua`, `*.client.lua` and `*.module.lua` (and `*.blocks.json`) into Instances in ServerScriptService, StarterPlayerScripts and ReplicatedStorage. `project.scripts` is a list of `ScriptFile { instance, path, source }` in `script_order` (modules, then Scripts, then LocalScripts, each by path). Every host starts that list with `ScriptRuntime::start_script`.
- `world_file.rs` refuses a script node (`WorldError::Script`), and `write_prefab` and `saved_children` leave scripts out.
- Two things already put scripts elsewhere in the tree. UI graphs compile to a LocalScript under their ScreenGui and join the same `ScriptFile` list (`ui_graphs.rs`). A cloned script starts once it's somewhere scripts run (`ScriptRuntime::start_clones`): a Script under the Workspace or ServerScriptService, a LocalScript under the Workspace, StarterPlayerScripts or Players. Clones get their code from the original's entry in `started`.
- A joined device loads the same game and starts its LocalScripts by Instance ID (`net::ScriptCode`).

So the runtime already runs scripts inside objects. What's missing is the file format, the loader and the editor.

## Design

### Format

- Script classes get one new property, **`SourceFile`**: a project path (`objects/Door/Open.server.lua`). It's empty for the scripts made from `scripts/`, which are never saved, so nothing changes for them.
- `world.json`, level files and prefab files may hold a script node. It has class, name and `SourceFile`, and nothing else is saved (no code):
  ```json
  { "class": "Script", "name": "Open", "properties": { "SourceFile": "objects/Door/Open.server.lua" } }
  ```
  A script node without a `SourceFile` stays an error. The file's ending must match the class: `.server.lua` for a Script, `.client.lua` for a LocalScript, `.module.lua` for a ModuleScript, or the `.blocks.json` forms.
- **Where the files live: a top-level `objects/` folder** (proposed default, decision 1). `scripts/` keeps meaning "a script in a service", and its scan never sees `objects/`. A reference may point anywhere in the project. Asset Check warns when one points into `scripts/`, because that file would also run once as a service script.
- A file written with a script node says `format_version: 2`. A file without one keeps writing 1 and stays byte-for-byte the same. An older engine then says "update Axcenity to open it" rather than the misleading "scripts can't be saved". Even today, an older engine refuses a script node rather than dropping it silently.

### Loading and start order

- The project loader walks the loaded tree (Workspace, ReplicatedStorage, ServerStorage, StarterGui and the rest) for script Instances with a `SourceFile`. It reads each file once, even when many copies share it, translates block scripts as `scripts/` does, and adds one `ScriptFile` per Instance. Hosts keep starting the one list, so the player, the server, the test runner and editor Play can't differ.
- Order:
  1. Every ModuleScript, services first and then objects in tree order. They're only registered, so `require` works whatever starts next.
  2. Service Scripts by path, as now.
  3. Object Scripts in tree order.
  4. LocalScripts the same way.

  This is the order G2 proposed in the gap analysis, and a golden test fixes it.
- **Where an object script runs** follows `start_clones`:
  - A Script runs under the Workspace or ServerScriptService.
  - A LocalScript runs under the Workspace, StarterPlayerScripts or Players, or under a ScreenGui (as UI graphs do).
  - Anywhere else, such as a template in ServerStorage or ReplicatedStorage, it's **kept but not started**. A new `ScriptRuntime` call records its code as `started` does, so a `Clone` of the template starts once it's placed, through the existing clone path. Without it, a cloned template would have no code.
- A missing or unreadable file is a load problem (`Project::problems`). The Instance stays and does nothing, so saving doesn't lose the reference.

### Copies, prefabs and packages

- Each copy has its own Instance and `script.Parent`, and copies share the file: one door script, many doors. A prefab's copies get their scripts from the prefab file through `Prefabs::refresh`, with `SourceFile` overridable like any property.
- `Clone` and Duplicate keep `SourceFile`. **Make Unique** (Explorer, on a script) copies the file next to the original (`Open 2.server.lua`) and points this copy at it (decision 2).
- `.axpkg` export and import carry the files that `SourceFile` points at (`asset_refs::find_references` learns the property). Build Game packs `objects/`, translating block scripts there the way `scripts/` is handled.

### Editor

- The Explorer shows scripts under objects. Opening one opens its file's tab, as service scripts do.
- **Insert > Script** (and LocalScript, ModuleScript) on a selected object writes `objects/<model or part name>/<Name>.server.lua` from the usual template and adds the Instance. It's one undo step, through `History::record_world_and_files`.
- Deleting the Instance keeps the file. Asset Check lists files nothing references any more, and Unused offers them.
- Rename or Move… on the file rewrites every `SourceFile` that points at it (`asset_refs::plan_rename`). Renaming the Instance doesn't rename the file.
- Live reload in Play reruns every Instance that uses a changed file (`reload_script` per Instance). Find in Scripts and Ctrl+K search include `objects/`.
- Properties shows `SourceFile` as a file picker limited to the matching ending.

### Multiplayer and streaming

- Server Scripts never run on a replica (`client_only` already skips them).
- A joined device loads the same game, so object LocalScripts reach it in its `ScriptCode` list at the same IDs.
- A LocalScript the server **clones and places at run time** reaches the replica as a replicated Instance with no code. The replica starts it from its own copy of the file its `SourceFile` names. That's new: today `ScriptCode` covers only scripts from load.
- With streaming on, a LocalScript inside a model that hasn't streamed in starts when the model does. On the server everything is loaded, so nothing changes there.

### Toolbox models (with G1)

A model from the Toolbox that carries scripts runs code from the internet. On insert, the editor lists its scripts and asks before keeping them. Declined scripts are removed along with their files. The licence record from G1 covers the files.

## Compatibility

- Projects without object scripts load, save and run exactly as before. Their files keep `format_version` 1.
- No existing script moves. Moving one into an object is the developer's choice: Explorer drag from ServerScriptService onto an object moves the file into `objects/` (a later PR, not required).

## Tests

- World, level and prefab round trips with script nodes, plus the version 1/2 rule.
- A prefab placed twice runs its script per copy, each with its own `script.Parent`.
- A template in ServerStorage doesn't run until a `Clone` lands in the Workspace.
- The start order is the same in the player, server, test runner and editor Play (golden).
- A replica doesn't run server Scripts and does run an object LocalScript.
- A run-time clone of a LocalScript starts on a replica.
- Package export brings the files. Rename or Move rewrites references.
- Asset Check reports missing, unused and `scripts/` references.
- Live reload reruns every copy.
- Build Game packs `objects/` (block scripts translated).

## Plan (one PR each)

1. **Format and runtime**: `SourceFile`; script nodes in world, level and prefab files with the version rule; the loader's tree walk and order; keeping uncloned templates' code. No editor UI yet, and an example project with a prefab door.
2. **Editor**: Explorer, Insert Script, tabs, Make Unique, `SourceFile` picker, live reload.
3. **References**: asset refs, Rename or Move, Find in Scripts, Asset Check, packages, Build Game.
4. **Multiplayer and streaming**: replicas start replicated LocalScripts by `SourceFile`; streamed models start theirs on arrival.
5. **Toolbox insert confirmation**, with G1's model step.

Effort is M–L in total (2–4 weeks), as estimated in the gap analysis.

## Decisions (defaults proposed; implementation starts with them unless Paul objects)

1. **Folder**: object scripts' files go in a top-level `objects/` folder that the `scripts/` scan never reads, rather than a subfolder of `scripts/` or "any file a node references is skipped by the scan". This keeps each file's role clear: deleting the last reference never turns a door script into a global server script.
2. **Copies share their file** (like Unity's script assets), with Make Unique to split one off, rather than each copy getting its own file.
3. **Property name `SourceFile`**: Roblox's `Source` holds the code itself, so reusing that name for a path would mislead.
