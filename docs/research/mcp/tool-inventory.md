# MCP tool inventory (proposal)

The proposed tools, what each one does, its safety class, its parameters, and what it returns. Conventions are in [architecture.md §5](architecture.md#5-tool-conventions). Engine coverage is in [engine-capability-map.md](engine-capability-map.md).

Design rules this list follows:

- **One verb per tool, one noun family per prefix**: `object_*`, `script_*`, `asset_*`. A small model picks by name.
- **No tool for something the engine can't do** ([capability map §3](engine-capability-map.md#3-missing-capabilities-dont-fake-them)).
- **High-level tools are thin**: they call the same engine operations as the low-level ones (architecture §4), so results can't differ.
- **Three profiles** so a small model never sees tools it won't use. A profile only hides tools; every tool behaves the same in every profile.

| Profile | Tools | For | Definition budget |
|---|---|---|---|
| `lite` | 11 | local models, 8–32k context | ≤ 2,000 tokens |
| `standard` (default) | 21 | Claude Code, Cursor, Codex | ≤ 4,500 tokens |
| `full` | 28 | advanced agents that need everything | ≤ 6,500 tokens |

Budgets are enforced by a test that counts the serialized `tools/list` (architecture §6.4).

Class: **R** read-only, **A** additive, **M** modify (reversible), **D** destructive/high-impact (preview + `confirm`). A tool whose class depends on its arguments says so; the server classifies **each call** by what it will do, not by the tool name.

## Shared parameter names

Every tool uses the same names for the same things. A small model learns them once.

| Name | Type | Meaning |
|---|---|---|
| `id` | string | One object: `"17"` (session ID from an earlier result) or a path `"Workspace/House/Door"`. |
| `ids` | string[] | Several objects, same forms. |
| `parent` | string | Same forms as `id`; `"Workspace"` by default where it applies. |
| `props` | object | Flat map `PropertyName → value`, values in the `world.json` format: `[x,y,z]` vectors, `[r,g,b]` 0–1 colors, enum names as strings (`"Neon"`), numbers, booleans, strings, project paths. |
| `path` | string | A project-relative file path with `/` (`scripts/door.server.lua`). |
| `detail` | `"min"` \| `"std"` \| `"full"` | How much to return. Default per tool. |
| `cursor` | string | Opaque paging or change cursor from the last result. |
| `limit` | integer | Max rows; each tool has a default and a hard cap. |
| `confirm` | string | The token from a D preview, to go ahead. |

## Lite profile (11 tools)

### `project_info` · R
Project summary: name, engine version, protocol/toolkit version, open level and levels, object/script/asset counts, problems count, permission mode, missing capabilities, and the change cursor. No parameters (optional `detail`).

```
→ {}
← {"name":"Hello Part","engine":"0.42.0","toolkit":"1.0","level":"Main","levels":["Main","Cave"],
   "counts":{"objects":214,"scripts":6,"assets":38},"problems":2,"mode":"edit","cursor":"c981"}
```

### `scene_inspect` · R
Browse or search the scene. Without filters: the tree under `root` (default `Workspace`) to `depth` (default 2). With any filter: a flat list of matches with paths. One line per object.

Params: `root` (id or `"selection"` in live mode), `depth`, `class`, `name` (substring, case-insensitive), `tag`, `near` (`[x,y,z]`) + `radius`, `limit` (default 50, cap 500), `cursor`.

```
→ {"class":"PointLight","near":[0,0,0],"radius":40}
← 3 of 3
  41 PointLight "Lamp"  Workspace/Street/Post1/Lamp
  44 PointLight "Lamp"  Workspace/Street/Post2/Lamp
  90 PointLight "Fire"  Workspace/Camp/Fire
```

Large scenes: children beyond `limit` collapse into `… 312 more Parts` with a cursor. Repeated subtrees (same class and name) fold into `Fence ×40`.

### `object_get` · R
Properties of one or more objects. `detail:"min"` = class, name, parent; `"std"` (default) = **non-default** properties only, rounded to 3 decimals; `"full"` = every property, attributes, tags, prefab link, references to and from. `props` (string[]) narrows to named properties. `since` (cursor) returns only properties changed since then.

```
→ {"id":"17"}
← 17 Part "Door" in 12 (Workspace/House)
  Position [4,3.5,0]  Size [4,7,0.5]  Color [0.45,0.3,0.2]  Material "Wood"  Anchored true
```

### `object_create` · A
Create one object with the same defaults and naming as the editor's Insert. `kind`: an Insert name (`Block`, `Ball`, `Cylinder`, `Wedge`, `Model`, `Folder`, `PointLight`, `Script`, `Sound`, `Frame`, … — `docs` lists all). Optional `name`, `parent`, `position`, `size`, `color`, `material` (built-in name or a `.material.json` path), `props`.

The convenience parameters (`position`, `size`, `color`, `material`) are the high-level layer: they're spelled out because they're used in most calls. Anything else goes in `props`.

```
→ {"kind":"Block","name":"Wall","position":[0,5,10],"size":[20,10,1],"material":"Brick"}
← created 233 Part "Wall" in Workspace · undo "AI: Create Wall"
```

### `object_set` · M
Change one or many objects (same change to each): `props`, `name`, `parent` (reparent), `attrs` (map, `null` removes), `tags_add`, `tags_remove`. Returns what changed, not the object. Locked objects are refused unless `props` sets `Locked:false`.

Becomes **D** (preview) when it touches more than 50 objects or reparents a service child.

```
→ {"ids":["41","44"],"props":{"Brightness":3,"Color":[1,0.8,0.6]}}
← changed 2 objects: Brightness, Color · undo "AI: Set 2 objects"
```

### `object_delete` · D (above threshold) / M
Delete objects (the editor's Delete: detach, undoable). Under the threshold (≤ 5 objects, no scripts, no services' children) it runs directly. Otherwise it returns a preview (what goes, including descendants and scripts, and what still points at them) and a `confirm` token valid for this exact request. Scripts always preview, as the editor always asks before deleting a script.

```
→ {"ids":["12"]}
← preview: deletes Model "House" with 38 descendants incl. 1 Script (scripts/door.server.lua).
  2 references point into it (Workspace/Gate.Target). confirm:"k7Qp"
→ {"ids":["12"],"confirm":"k7Qp"}
← deleted 39 objects · undo "AI: Delete House"
```

### `docs` · R
Reference lookup, from the engine's own docs (`script/src/definitions.rs`, `docs/`). One of: `class` (properties with types, defaults and one-line meaning; events), `class` + `property` (one property in full), `kind:"insert"` (every `object_create` kind), `topic` (a docs topic file's section), or `query` (search; returns ≤ 5 snippets with their topic names).

```
→ {"class":"PointLight"}
← PointLight (Light, Instance) — a light shining in every direction from its part.
  Brightness number 1 · Color color [1,1,1] · Range number 8 (studs) · Shadows bool false · Enabled bool true
```

### `script_read` · R
A script's text with line numbers, or a range (`from`, `to`). `path` or the Script object's `id`.

### `script_write` · A / M
Create a script (`path` that doesn't exist, plus `side`: `server` | `client` | `module`, and `parent` for where its Script object goes) or change one: `text` (whole file) **or** `edits` (list of `{find, replace}` exact-match pairs, each must match once — the cheap way to change a large file). Returns a syntax check (`ScriptContext::check_syntax`) and side warnings (`side_warnings.rs`). One file undo step.

```
→ {"path":"scripts/door.server.lua","edits":[{"find":"wait(2)","replace":"task.wait(2)"}]}
← edited scripts/door.server.lua (1 change) · syntax ok · 0 warnings · undo "AI: Edit door.server.lua"
```

### `project_check` · R
One pass of the existing checks: asset check (missing files, invalid references, import failures), script syntax, side warnings, prefab and level problems, plus Play errors from the last run. Compact list, worst first, each with where it is and a suggested fix (from `asset_fix::plan_fix` when there is one). `scope`: `all` | `assets` | `scripts`.

### `history` · R / M
What the AI (and the user, in live mode) changed, and undo.

- `action:"list"` (default): recent undo steps with labels and who made them (`AI (Claude Code)`, `user`).
- `action:"changes"`, `since`: objects created, removed and changed since a cursor, compact.
- `action:"undo"` / `"redo"`, `steps` (default 1): only when the top step(s) are this client's; otherwise explains which user step is on top. The user can always undo anything in the editor.

## Standard profile (+10 = 21 tools)

### `object_duplicate` · A
`ids`, `count` (default 1), `offset` (`[x,y,z]` per copy). Internal references point into the copy (`World::duplicate`).

### `material_edit` · A / M
- `action:"create"`: `name`, PBR values (`color`, `metalness`, `roughness`, `emission`, `transparency`), map paths (`albedo`, `normal`, `roughness_map`, …), optional `base` (another material file). Writes `assets/materials/<name>.material.json`.
- `action:"set"`: change values in an existing material file (`path`). Affects every part using it → preview when used by more than 50 parts.
- `action:"assign"`: `ids` + `path` (sets `MaterialVariant`, Color white, as the editor's drop does).

### `env_set` · M (high-level)
One call for the look of the world: `preset` (`day`, `sunset`, `night`, `overcast`, `foggy`), `time` (0–24), `latitude`, `fog` (`{color,start,end}` or `false`), `clouds` (0–1 cover or `false`), `weather` (`none`, `rain`, `snow` + `intensity`), `sky` (image path), `bloom`, `exposure`, `shadows`. Creates Atmosphere/Clouds/Weather/effects in Lighting when needed, changes them otherwise. One undo step. Returns what it set.

### `terrain_edit` · A / M / D
`action`:
- `generate`: `preset` (from `terrain::generate` presets), `size`, `seed`, `height`. D when a landscape exists (replaces it; preview shows sizes).
- `brush`: `tool` (`raise`, `lower`, `smooth`, `flatten`, `noise`, `paint`), `at` (`[x,z]` or `[x,y,z]`), `radius`, `strength`, `layer` (for paint). `path` (list of points) for a stroke.
- `erode`: `kind` (`hydraulic`, `thermal`), `amount`.
- `foliage`: `layer`, `density`, `area`.
Each call is one terrain undo step.

### `anim_edit` · A / M
- `animate`: `id` → Animator + clip + Animation (the editor's Animate).
- `key`: `clip` (path), `target` (object path relative to the animated root, `"."` for the root), `property`, `time`, `value`, `interp` (`linear`, `constant`, `cubic`, + easing). `unkey` removes one.
- `clip_info`: tracks and keys, compact.
- `state` / `transition` / `param` (state-machine files): add/change/remove one item.

### `asset_find` · R
Project assets by `kind` (`model`, `texture`, `material`, `audio`, `prefab`, `animation`, `script`, `stylesheet`, `level`), `name`, `unused:true`, `missing:true`. `detail:"full"` adds size, references (from `asset_refs::find_references`) and, after the Toolbox lands, source and licence.

### `asset_import` · A
Copy a model, texture, audio file or package into the project from an **allowed** folder (configured by the user), place it if it's a model (`parent`, `position`). Undo removes the copied files. Returns the new asset paths and created objects.

### `game_run` · M
- `mode:"play"`: Play for `seconds` (default 3, cap 60), optional `inputs` later; returns script errors and the last `max_lines` (default 30) of output, frame-time summary, then Stop. Play changes are discarded as in the editor. Live mode: the user sees it play.
- `mode:"tests"`: run game tests (`testing::Runner`), optional `file`/`name`; returns pass/fail counts and failures with `file:line`. Says when unsaved changes aren't in the run.

### `batch` · by contents (A/M/D)
Several operations as **one transaction and one undo step**. `ops`: list of `{"do":"object_create","args":{…},"as":"wall"}`; later args can name earlier results as `"$wall"`. Any of the write tools above (not `batch`, `game_run` or `project_save`). Checked first (every op validated, references resolved, class = the highest of its ops; D ops need the batch's `confirm`), then run in order on the edit world; the first failure rolls everything back and reports which op failed and why. Caps: 100 ops, 2,000 created objects.

```
→ {"ops":[
    {"do":"object_create","args":{"kind":"Model","name":"Hut"},"as":"hut"},
    {"do":"object_create","args":{"kind":"Block","name":"Floor","parent":"$hut","size":[8,1,8]}},
    {"do":"object_create","args":{"kind":"PointLight","parent":"$hut","position":[0,4,0]}}]}
← ok 3 ops · created 301 Model "Hut", 302 Part "Floor", 303 PointLight · undo "AI: Batch (3)"
```

### `project_save` · D
Save the world, levels and open files. Preview names the files that will be written; `confirm` saves. In permission mode `full` it saves without preview. Headless sessions need it; in live mode the user's own Save is normal.

## Full profile (+7 = 28 tools)

| Tool | Class | Does |
|---|---|---|
| `level` | R/A/M | List, create, open (swaps the edit world; refuses with unsaved changes unless confirmed), set start level. |
| `prefab` | A/M/D | Save selection as prefab, place, list overrides, revert; `apply` rewrites the prefab for every copy → D. |
| `solid_op` | A/M | Union, Negate, Intersect, Separate on `ids`. |
| `style_edit` | M | Read and change CSS rules in a stylesheet by selector (`selector`, `props`), or create a stylesheet. |
| `project_settings` | M/D | Input map, collision groups, string table entries, developer console, loading screen. Project-wide → D for removals. |
| `asset_manage` | M/D | Move/rename with reference updates (`plan_rename`) · delete (D, preview lists users). |
| `render_stats` | R | Frame and pass times, draws, triangles, GPU memory (live editor or last Play). |

## Deferred (listed so nobody adds them by accident)

| Tool | Why later |
|---|---|
| `view_capture` | Needs a GPU frame in headless mode and costs image tokens; high value for multimodal agents. After Phase 4. |
| `mesh_edit` | Model Studio ops need face/edge selection by index; revisit with `view_capture`. |
| `asset_search_online`, `asset_get` | Wrap the Toolbox backend (PR #90, Toolbox thread) once it's on `main`. |
| `data_edit` | Data tables, dialogues, string table: simple JSON files, after the core. |
| `graph_edit` | Block scripts, UI graphs, behavior trees, material graphs: agents should write Lua; graphs later if asked. |
| `run_lua` | Not planned (decision D6). |

## Resources and prompts

Resources mirror tools for clients that use them (Claude Code, Cursor and VS Code read resources; several clients don't, so nothing is resource-only):

| URI | Same as |
|---|---|
| `axcenity://project` | `project_info` |
| `axcenity://scene/{level}` | `scene_inspect` depth 2 |
| `axcenity://object/{id}` | `object_get` std |
| `axcenity://class/{name}` | `docs {class}` |
| `axcenity://docs/{topic}` | `docs {topic}` |
| `axcenity://script/{path}` | `script_read` |
| `axcenity://problems` | `project_check` |

Prompts (optional, two to start): `axcenity_build` ("plan and build a scene from a description using batch") and `axcenity_fix` ("run project_check and fix what it finds"). Prompts are where long guidance lives, so tool descriptions stay one sentence.
