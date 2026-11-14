# Import formats and editor extensibility (P7): investigation and stages

Priority 7 of Paul's list of 2026-11-14: "investigate USD and Alembic support, in addition to the existing glTF, GLB, and FBX pipeline. Mesh import presets and more advanced asset conversion are also worth considering", and "improve the existing Lua plugin API first. Native extensions should remain deferred until there are concrete use cases that cannot be handled through Lua." This file records what main has, what each format would take, the decisions, and stages P7.1–P7.6. Each stage is its own PR. How importing works today: [docs/editor/import.md](../editor/import.md); the plugin API: [docs/editor/plugins.md](../editor/plugins.md).

## What main has (14 Nov 2026)

| Area | Today |
|---|---|
| Model formats | glTF 2.0 (`.gltf`, `.glb`) through the `gltf` crate, FBX (binary and ASCII, every version) through `ufbx` 0.11. Both are read at load time by `assets` (`model.rs`, `model/fbx.rs`), in the editor and in built games alike, into one `Model` (triangles fitted into the unit cube, materials and maps, rig, `_LOD`/`_COL` objects) |
| Animation sources | FBX takes (baked per bone); glTF animations (read as takes) |
| Import | One Import… and drag and drop: the Import dialog (objects, Bring in as, Weld parts, materials and maps, clips and an Animator), copied into `assets/models/`, embedded textures copied out, one undo step |
| Import presets | Texture presets only (R12, `assets/src/import_presets.rs`): compression and largest size, per file (sidecar) or per folder (`import-presets.json`). Nothing remembers how a model comes in |
| Conversion | None: a file is read as the format it is, or refused |
| Plugin API | `plugin.ApiVersion` 2: toolbars, dock widgets, Selection, ChangeHistoryService, settings, project files (permission), the viewport mouse (Phase 54), and since G10 viewport drawing, handles, Properties rows, importers for file kinds the editor doesn't read, `plugin.AssetImported(path)` and Assets panel menu items |

## Formats

| Format | What it's for | Rust readers (crates.io, 14 Nov 2026) | Verdict |
|---|---|---|---|
| **USD** (`.usd`, `.usda`, `.usdc`, `.usdz`) | Scene interchange between DCC tools (Houdini, Maya, Blender, Omniverse); Apple's AR format (`.usdz`); layered scenes with references and variants | `openusd` 0.8.0 (MIT, pure Rust, no C++; reads and writes all three encodings; full composition engine: sublayers, references, payloads, variants, instancing; passes the AOUSD Core Specification 1.0 parsing and composition suites) with `openusd-schemas` 0.8.0 (MIT) for typed Mesh, Xform, Material and Skel views. MSRV 1.96, so it builds on our pinned 1.97. The C++ bindings (`usd`, `rust-usd`) need Pixar's USD built and installed | **Build** (P7.2), converted to glTF on import |
| **Alembic** (`.abc`) | Baked geometry caches: cloth, fluids, destruction, crowds, as per-frame vertex positions (Unreal's Geometry Cache, Unity's Alembic package) | None usable: the `alembic` crate is an empty 2020 placeholder (the other `alembic-*` crates are an unrelated network tool). The reference library is C++ (BSD-3, Ogawa container, optional HDF5) | **Deferred** (P7.5): needs a new reader *and* a geometry-cache playback system the engine doesn't have |
| **OBJ** (`.obj` + `.mtl`) | The oldest static-mesh interchange; scans, kitbash packs, CAD exports | `ufbx`, which we already ship, reads it (`FileFormat::Obj`, `.mtl` passed as bytes) | **Build** (P7.1), read directly like FBX |
| STL, PLY | 3D printing and scans (no materials) | Small crates exist (`stl_io`, `ply-rs`) | Not now: no request, and both convert losslessly to OBJ or glTF in any DCC tool |
| `.blend` | Blender's own file | Only by running Blender | Not now: R15.4's Edit in Blender already round-trips through `.glb` |

### USD: read at load time, or convert on import?

| Option | For | Against |
|---|---|---|
| Read `.usd*` at load time, as FBX is | The file in the project is the user's file; edits in the DCC tool show up through the watcher | `openusd` is ~143k lines (plus `logos`, `lz4_flex`, a second `zip` major version): every build of the player and the server compiles it, for a format no shipped game needs. Every load pays composition. Cooking, LODs, `MeshId` object picks, Model Studio and the Blender round trip would each need a USD path |
| **Convert to `.glb` when imported** (Godot's approach for `.blend` and FBX) | Only the editor links `openusd`; after import the model is an ordinary `.glb`, so cooking, `ax://` IDs, LODs and collision pieces, object picks, the Import dialog's tabs and preview, Model Studio and built games all work unchanged. A converter written once also serves later formats (Alembic's first frame, STL) | The project holds the converted `.glb`, not the `.usd`: changes in the DCC tool come in by importing again. Data glTF can't carry in our reader (UsdSkel animation, variants beyond the selected one) is lost unless converted on purpose |

**Decision (D1): convert.** A new editor-only crate, `convert` (`axcenity-convert`), turns a USD stage into a `.glb`: composed with its references and payloads loaded and default variants selected, each `Mesh` prim (triangulated, with normals, the first `st` primvar, and per-face `GeomSubset` materials) under its `Xform` path as a glTF node with the same name (so the Import dialog's Objects tab shows the USD tree), `metersPerUnit` and `upAxis` applied (glTF is Y up, meters), and `UsdPreviewSurface` materials as metallic-roughness: diffuse color, metallic, roughness, normal, occlusion, emissive and opacity, with their texture files embedded in the `.glb` (separate metallic and roughness maps are packed into one texture's blue and green, opacity into the color's alpha). The editor converts before the Import dialog opens, so the rest of importing is as for a `.glb`, and the sidecar notes the source file (`"converted_from"`). `.usdz` textures come out of the archive the same way. Out of P7.2: UsdSkel and animation (P7.6), lights, cameras, instancing beyond flattening it, and writing USD.

Recorded as [ADR-0027](../../wiki/Architecture/Architecture-Decisions/ADR-0027-Convert-New-Model-Formats-To-glTF-On-Import.md). OBJ is the exception that proves the rule: `ufbx` already reads it in the player, so it costs nothing to read it directly.

### Alembic: why deferred

An Alembic file is almost always an animated vertex cache, and its value is playing that cache back. The engine has skinned meshes but no per-frame vertex streams: we'd need (1) an Ogawa reader in Rust (the container is simple, but the schemas, sample indexing and topology-varying meshes are a few thousand lines), (2) a cooked cache format with compression, (3) a `GeometryCache` Instance with streaming playback, GPU upload per frame and Sequencer tracks, and (4) networking rules for when it plays. Only the first frame as a static mesh would be cheap, and every tool that writes Alembic also writes FBX, glTF or USD for that. Per the development policy (no infrastructure before it's needed), Alembic waits for a real workflow, such as a cutscene with a simulated cloth or destruction cache. When it comes, the reader lives in `convert`, and the first-frame import reuses D1.

## Mesh import presets

Texture presets (R12) say how a texture is cooked. A model has no equivalent: every import starts from the Import dialog's defaults, so a team importing fifty props re-ticks Weld parts fifty times, and nothing makes a centimetre-scale or Z-up export come in right.

**Decision (D2):** model presets join the texture presets in `import-presets.json`, as `"model_presets"`, and are picked in the Import dialog (a Preset dropdown above the tabs; the last one used is remembered per project). A preset holds the choices the dialog already has plus four new ones:

| Setting | Values | Today |
|---|---|---|
| Bring in as | A Model, Separate MeshParts, One merged MeshPart, or By file (one object: merged; several: a Model) | By file |
| Weld parts | on/off | off |
| Add an Animator (animated files) | on/off | on |
| **Scale** | a factor (0.01 for a centimetre export) | 1 (FBX and USD units already applied) |
| **Up axis** | Y, Z | Y (glTF's; FBX and USD say their own) |
| **Anchored** | on/off | off |
| **Collision fidelity** | Default, Box, Hull, PreciseConvexDecomposition | Default |

Built in: **Default** (today's behavior), **Static prop** (merged, Anchored, Hull), **Building kit** (a Model, welded, Anchored, PreciseConvexDecomposition), **Character** (one MeshPart, Animator, Box). The dialog's **Save as preset…** writes the current choices under a name. Folder rules don't apply (a model comes from outside the project), so a preset is picked, not inferred; a plugin can pick one too (P7.4's `ModelImported` can post-process).

## Plugin API: what's missing

Compared with Roblox Studio plugins and Godot's `EditorPlugin`, the G10 API covers the viewport, Properties and assets. What plugin authors still can't do:

| Gap | Roblox | Godot | Use case |
|---|---|---|---|
| Commands with shortcuts, found in Ctrl+K | `plugin:CreatePluginAction` (bindable shortcut) | `add_tool_menu_item`, shortcuts | "Align selected (Ctrl+Shift+A)" without a toolbar click |
| Explorer right-click items | `plugin:CreatePluginMenu` | context menu plugins | "Convert to prefab" on a Model |
| Changing what an import made | — (Studio has no hook) | `EditorScenePostImport` | name parts, add attributes and tags, set collision groups for a whole pack |
| Saving state before a reload | `plugin.Unloading` | `_disable_plugin` / `_exit_tree` | keep a half-made layout across hot reload |
| Opening a script at a line | `plugin:OpenScript(script, line)` | `EditorInterface.edit_script` | a linter plugin jumping to a problem |
| A toast or notice | `StudioService` notifications | `EditorToaster` | "Aligned 12 parts" |

**Decision (D3):** API 3 adds exactly those six: `plugin:CreateAction({ Id, Text, Shortcut, Tooltip })` (with `Triggered`; listed in Ctrl+K under the plugin's name, its shortcut working while editing unless the editor already uses it), `plugin:AddExplorerAction({ Label, Class, Tooltip })` (with `Activated(instances)`), `plugin.ModelImported(root, path)` (fires after an import's Instances are inserted, inside the import's undo step), `plugin.Unloading` (fires before a reload or stop; settings written then are kept), `plugin:OpenScript(script, line)` and `plugin:Notify(text)`. `plugin.ApiVersion` becomes 3; nothing is taken away. Still out: plugin-to-plugin calls, build or save hooks, script-editor analysis hooks and permissions beyond `files`, each waiting for a plugin that needs it.

### Native extensions stay deferred (D4)

Lua plugins run sandboxed, survive bad code (errors are caught and listed), reload on save and need no compiler. A native ABI (Godot's GDExtension, Unreal modules) would add a stable C ABI to keep across versions, code that can crash or compromise the editor, and per-platform binaries for every plugin. The condition for revisiting it: a concrete plugin that Lua can't serve after P7.4, because it needs (1) a native library (a vendor SDK, a codec), or (2) CPU work that's too slow in Luau with `--!native`, measured. Until then, such work goes into the engine itself.

## Stages

| Stage | What | Status |
|---|---|---|
| P7.0 | This investigation, ADR-0027, wiki | This PR |
| P7.1 | OBJ import: `ModelFormat::Obj` through `ufbx` (with the `.mtl` and its textures from the model's folder), in Import…, drag and drop, the Assets panel and the player | PR #351 |
| P7.2 | USD import: the `convert` crate (`openusd` + `openusd-schemas`), `.usd`/`.usda`/`.usdc`/`.usdz` converted to `.glb` before the Import dialog opens, `"converted_from"` in the sidecar | PR #362 |
| P7.3 | Mesh import presets: `model_presets` in `import-presets.json`, the Preset dropdown and Save as preset… in the Import dialog, Scale, Up axis, Anchored and Collision fidelity | PR #374 |
| P7.4 | Plugin API 3: actions with shortcuts in Ctrl+K, Explorer menu items, `ModelImported`, `Unloading`, `OpenScript`, `Notify` | Planned |
| P7.5 | Alembic | Deferred (see above) |
| P7.6 | USD skeletons and animation (UsdSkel to a rig and takes) | Later, after P7.2 is used |
