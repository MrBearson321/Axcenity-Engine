# Asset metadata

Every asset, whether generated, imported from a provider or modelled by hand,
has one **asset record**: a JSON object that every subsystem reads and
writes. Records for a pack live in its **asset index**
(`packs/<pack>/asset_index.json`, setting `metadata.index_file`), which also
keeps each asset's version history.

Code: `asset_metadata.py` (schema, IDs, validation, serialisation; no `bpy`),
`asset_index.py` (the index, duplicates, dependencies, history; no `bpy`),
`asset_records.py` (builds records from Blender objects and stores the
values that belong on the object), `pack_library.py` (every pack's index read
together; no `bpy`). CLI: `record`, `assets`. Sidebar: Pack Assets.

## Record (`format: "pipeline-asset"`, `format_version: 1`)

| Field | Type | Meaning |
|---|---|---|
| `id` | string | `<pack_slug>/<AssetName>`, e.g. `general_store/SM_GS_Wall_BrickWindow`. Assigned once and stored on the object (`pipeline_asset_id`), so it survives renames. Same as the Axcenity manifest `id` |
| `name` | string | The object's current name |
| `pack` | string | Pack name (`pack.name`) |
| `category` / `subcategory` | string | Category from the config; optional subcategory |
| `tags`, `description` | list, string | From `setup_asset` |
| `kind` | enum | `generated` (has `generation`), `imported` (has a provider source), `manual` |
| `status` | enum | `draft` → `generated` → `validated` → `exported`; `failed` (validation or QA errors), `deprecated`. Only a QA pass (`cli.py -- qa`, `qa.run`) sets `validated`. `validated` and `exported` drop back to `generated` or `draft` when the content fingerprint changes. `deprecated` is left only through `restore`: recording, QA, jobs and exports keep it |
| `version` | int | Starts at 1; the index raises it whenever `fingerprint` changes |
| `fingerprint` | string | Hash of geometry, materials, generation, collision, LODs, and the billboard and rig (only when there is one, so older records keep theirs): what makes it a different asset |
| `geometry` | object | `dimensions_m`, `bounds_min`, `bounds_max` (local, metres, Z up), `pivot` (category rule), `triangles`, `vertices`, `uv_maps`, `fingerprint` (mesh only, for duplicate detection) |
| `materials` | list | `{name, preset, library}` per slot (`library` = provider key or null) |
| `textures` | list | Image files the materials read, as stored in Blender |
| `collision` | object | `type` (BOX, CAPSULE, CONVEX, CONVEX_PARTS, MESH or null), `objects` |
| `physics` | object | `physics.describe`: `body` (static/dynamic), `density`, `friction`, `elasticity`, `friction_weight`, `elasticity_weight`, `massless`, `can_collide`, `can_touch`, `source` (material kind), `volume_m3`, `mass_estimate`, `engine` (`strategy` hull/colliders/none, `colliders`, `note`) |
| `lods` | list | `{name, level, triangles}` |
| `billboard` | object | Empty without one: `name`, `kind` (`cross`, `impostor`), `resolution`, `triangles`, `textures` (kind -> path relative to the pack), `source` (hash of the mesh and material looks it was rendered from), `frames` (`{yaw, uv}`) |
| `rig` | object | Empty without one. A root (`rigging`): `role: "root"`, `root` (name), `parts` (name -> `joint`, `parent`, `position_m`, `rotation` 3x3, at rest in the root's space), `joints` (`name`, `kind` hinge/prismatic/cylindrical, `turn`, `slide` (`limits`, `actuator` none/motor/servo, `speed`, `target`, `max_force`; degrees or metres), `drive` constraint/clip, `clip` (`keys` of seconds and value, `loop`, `auto_play`), `visible`, `break_force`, `break_torque`, `part`, `parent`, `pivot_m`, `axis`), `ids` (part name -> asset ID). A moving part: `role: "part"`, `root` (ID), `joint`. A root's `dependencies` has an `asset:` entry per part. Posing a joint changes nothing here |
| `sockets` | list | Socket dicts (`name`, `pos`, `dir`, `tag`) |
| `seed` | int or null | `pipeline_seed` (per-object material variation; `variation.batch` gives each variant its own) |
| `generation` | object or null | `{generator, generator_version, params, seed, profile}`: enough to rebuild the asset exactly. Written by generators with `asset_records.set_generation`. `params` already hold the profile's changes, so `profile` is only its name. Present when used: `settings` (what the profile sets outside the mesh: `collision`, `texture_limit`, `optimisation`, see `lod_rules`), `variation` (`{of, batch_seed, index}` on variants made by `variation.batch`) and `nodes` (Geometry Nodes steps realised on the mesh, in order: `{group, group_version, inputs, seed}`; object and collection inputs are names) and `split` (moving parts cut off the mesh, in order: `{part, asset, category, selector}`; a rebuild repeats them and puts the joints back). A part cut off a generated asset has `generator: "rig_split"` and is rebuilt through that asset. A hand-made mesh with realised steps has `generator: "node_stack"` and cannot be rebuilt by a generator; `geonodes.replay` repeats its steps on that mesh |
| `assembly` | object | Only on structures built from an assembly spec: `pieces`, `connections`, `open_sockets`, `empty_slots`. Their `category` is `assembly.category`, `generation` is `{generator: "assembly", params: {spec}, seed}`, `geometry.fingerprint` is a hash of the layout, and `dependencies` has an `asset:` entry for every piece used |
| `environment` | object | Only on landscapes built from a spec (`terrain.py`): `samples`, `heights` (hash of the saved heights), `layers`, `roads`, `placed`, `foliage`, `structures`, `rules` (placed per rule). `category` is `terrain.category`, `generation` is `{generator: "environment", params: {spec}, seed}` (roads drawn as curves are stored as points), `geometry.fingerprint` hashes the heights, paint and layout, and `dependencies` has an `asset:` entry for every asset and structure used |
| `sources` | list | Library assets used (`key`, `license`, `url`, `authors`), from the library registry |
| `license` | string | Summary: `CC0-1.0` when every source is CC0, `mixed`, or `own` for no sources |
| `dependencies` | list | Typed references, `<type>:<ref>`: `asset:<id>`, `material:<name>`, `library:<provider:key>`, `texture:<path>` |
| `exports` | object | Per target, e.g. `{"axcenity": {"manifest": "...", "exported": "<time>"}}` |
| `previews` | object | The last preview set, empty until one is rendered: `fingerprint` (the record's when rendered; different from the current one means out of date), `settings` (hash of the preview settings and material looks), `rendered` (UTC time), `files` (view -> path relative to the pack folder, plus `thumbnail` and `turntable`, a list). Kept when the asset is recorded again |
| `qa` | object | The last QA pass, empty until one has run: `fingerprint` (the record's when checked; `validated` with a different one is flagged), `checked` (UTC time), `result` (`passed`, `warnings`, `failed`), `errors`, `warnings`, `notes` (counts), `report` (path relative to the pack folder). Kept when the asset is recorded again |
| `job` | object | The batch job working on the asset, empty when none has: `name`, `run`, `state` (`running`, `done`, `failed`, `blocked`), `step` (current or last), `error`. Not content, so it never makes a new version; kept when the asset is recorded again. An asset a job could not build at all is a placeholder record with an empty `fingerprint` and this block; its first real record is still version 1 |
| `deprecation` | object | Empty unless retired: `time`, `reason`, `replaced_by` (an asset ID or empty), `previous_status` and `fingerprint` (what `restore` needs). Not content, so no new version; kept when the asset is recorded again |
| `created`, `updated` | string | UTC ISO times; `created` never changes |

Required fields (`metadata.required`, default `id name pack category kind
status version`) must be present and non-empty; `asset_metadata.validate`
reports anything missing, unknown enum values and malformed IDs or
dependency strings.

## Index (`format: "pipeline-asset-index"`, `format_version: 1`)

```json
{"format": "pipeline-asset-index", "format_version": 1,
 "assets": {"<id>": {<record>}},
 "history": {"<id>": [{"version": 1, "fingerprint": "...", "status": "validated", "time": "...", "note": "..."}]},
 "favourites": ["<id>"]}
```

- `put(record)` keeps `created`, bumps `version` when the fingerprint
  changes, and appends a history entry on any version or status change
  (`metadata.history_limit`, default 50, newest kept).
- `duplicates()` groups different IDs with the same geometry fingerprint.
- `missing_dependencies(resolver)` lists `asset:` references not in the
  index, and other references the resolver says are missing.
- `dependency_order()` returns IDs with dependencies first and raises
  `PipelineError` naming the cycle if there is one.
- `find(category, status, kind, tag, text, license, favourite)` searches
  records. `tag` may be a list (all needed); `text` needs every word somewhere
  in the name, ID, description, tags, category, materials or generator;
  `license` is the record's summary, so `CC0` means nothing else is in it.
- `history_view(id)` is the history newest first, each row with `change`
  (`created`, `content`, `status`) and `previous_status`. `note` appears on
  deprecations and restores.
- `set_favourite(id, on)`, `favourites()`, `is_favourite(id)`: favourites
  belong to the index, so recording an asset again keeps them.
- `deprecate(id, replaced_by, reason)` and `restore(id)`. `restore` brings the
  old status back, unless that was `validated` or `exported` and the content
  has changed since.

## Across packs

`pack_library.open_packs(cfg)` opens every pack in `metadata.packs_directory`
(folders with a `pipeline.config.json`, not starting with `_` or `.`). A pack
that can't be read is left out and named in `problems`.

- `find(pack=None, **filters)` covers the current pack, `"all"`, or one by name.
- `dependency_view(ref)`: what an asset uses, by type, and what uses it,
  directly (`used_by.direct`) or through others (`used_by.indirect`), in any
  pack, with its variants.
- `affected_assemblies(ref)`: built `structures`, through any chain of
  dependencies, and spec files with the slots that have `named` it or that
  `may_pick` it by category or tag. A spec in another pack counts only if it
  uses the full asset name, since short names and categories there mean that
  pack's own pieces.
- `deprecate(ref, replaced_by, reason)` refuses a replacement that doesn't
  exist, is deprecated or is built from the asset. One that can't swap in is a
  warning. New assemblies leave deprecated pieces out and give their names to
  the replacement; `assembler.rebuild` still uses them, to repeat the layout.
- `asset_records.check` accepts `asset:` references to another pack, and warns
  about current assets that use a deprecated one.
- `merge_catalogs(library)` writes `blender_assets.cats.txt` over all packs, for
  one Asset Browser library on the packs folder. Blender reads the catalog file
  at a library's root, and each pack's catalogs are beside its `.blend` files.
  IDs are kept, and catalogs already in the shared file stay.

## In export manifests

`exports/manifest.json` entries and Axcenity manifests are built from the
record (`asset_records.collect`, read before export swaps in baked
materials) and carry `record: {id, fingerprint, kind}`. A manifest whose
fingerprint differs from the index's current one is out of date.

## Rules

- Add a field here before writing it. Readers must ignore unknown fields;
  bump `format_version` only when a field changes meaning, and teach
  `asset_metadata.upgrade` to read the old version.
- The record describes the asset; it does not replace the .blend. Rebuilding
  from `generation` must give the same `geometry.fingerprint`.
- `geometry.rebuild(record)` makes a generated asset again from its record.
  A generator raises its `version` whenever its output changes, and a rebuild
  from an older version logs a warning.
- Never edit `asset_index.json` by hand while Blender has the pack open;
  use `cli.py -- record` or `asset_records.record`.

## Scene objects (`scene/`)

The scene pipeline works on whole scenes, whose objects are often not assets
(demo instances, a terrain preview, supports it generated), so its facts live
on the object and in text blocks inside the .blend rather than in the asset
record:

| Where | Meaning |
|---|---|
| `pipeline_import_original` (object) | True on a downloaded file's untouched objects, parked in a hidden `IMP_*` collection under `IMPORTED_SOURCE`. Nothing edits them; analysis and validation never count them |
| `pipeline_import_digest`, `pipeline_import_derivative`, `pipeline_import_file` (collection) | The file's SHA-1 (importing the same bytes again reuses the collection), the derivative object's name, the file's name |
| `pipeline_import_derived_from` (object) | On a derivative: the digest of the file it was built from. `imports.importer.originals_of` finds the originals through it |
| `pipeline_import_scale` (object) | The uniform scale the enhancement step applied to reach a real-world size |
| `pipeline_import_budget` (object) | JSON of what an imported model is over budget by. Recorded, never reduced |
| `pipeline_import_textures` (object) | JSON `[{image, original, derived, from_px, to_px}]`: smaller copies written for exports; `enhance.export_textures` swaps them in for the length of an export |
| `pipeline_import_report` (object) | JSON of the last `EnhanceReport`, returned as it is when the same file is imported again |
| `pipeline_import_material` (material), `pipeline_image_digest` (image) | What makes two imported materials or images the same, so a kit's shared palette stays one datablock |
| `pipeline_acquire_key` (object) | The acquisition key: decisions and overrides are filed under it. Set it when the object's name without its copy number isn't the key |
| `pipeline_acquire_limitation` (object) | On a copy a generator made because nothing external qualified: what was tried and why it fell short |
| `pipeline_view_distance` (object) | Metres the object is meant to be seen from; realism validation reads it before the camera and the object's tier |
| `pipeline_instanced_from` (object) | The mesh it had before the optimise stage pointed it at a shared one; that mesh is kept and `restore` swaps it back |
| `pipeline_scene_class` (object) | The user's class for the object (`scene.classes.CLASSES`); classification confidence 1 |
| `pipeline_generated` (object) | JSON `{system, run, seed, key, sources: {name: fingerprint}, params, time}` on everything the pipeline made. `key` identifies the output (system, sources, parameters, seed), so a rerun finds and replaces it; `sources` hold each source's shape-and-placement fingerprint, so `is_stale` can tell when one changed |
| `pipeline_replaces` (object) | On a specialised system's output: the name of the object it stands in for. That object is hidden and analysis counts the replacement instead (`scene/specialised.py`) |
| `pipeline_placement` (object) | JSON `{rule, sink_m, tilt_deg, seat, seat_drop_m}` on pieces an environment build placed, so the structural checks know what was meant (a negative `sink_m` is a deliberate lift) |
| `pipeline_ground_z` (object) | The terrain height under the object, set by the materials stage; dirt with `ground: "contact"` measures height from it. `restore` removes it |
| `pipeline_scene_source`, `pipeline_scene_key`, `pipeline_texel_density` (material) | On a `__scene` copy: the material it was made from, what identifies its decision (a rerun rebuilds only when this changes), and the texel density it was made to (`game_ready.export_resolution` reads it) |
| `pipeline_vegetation_replaced` (object) | The run that hid this plant because the `environment` stage grew its own; while hidden it is left out of the analysis |
| `pipeline_vegetation_type` (object) | On a mesh in the masters collection: the vegetation layer it serves, when its name doesn't say |
| `pipeline_layer_names` (object) | JSON list on a built terrain's preview mesh: the layer names in splat order |
| `pipeline_moisture` (mesh attribute, 0..1) | Optional per-vertex moisture on a ground mesh, which species' moisture limits read |
| `pipeline_baseline` (text) | The untouched scene: per object its type, parent, world matrix, mesh and fingerprint, materials and their slot links, visibility, collections and `pipeline_*` properties; the collection tree; `kept` mesh copies (`<mesh>__baseline`, fake user) for meshes changed in place |
| `pipeline_changes` (text) | The change log: `[{time, action, system, run, objects, detail}]` for captures, generated supports, corrections (with `rule`, `before` and `after`), removals and restores |

A generated object that becomes an asset (recorded in the index) keeps its
`pipeline_generated` property; the record's `generation` block still describes
how to rebuild it.
