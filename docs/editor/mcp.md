# MCP server for AI clients

`axcenity-editor --mcp <project>` opens a project without a window and serves it to AI clients (Claude Code, Cursor, Codex, local models) over the Model Context Protocol, on stdin and stdout. Editor-only: the player and server never link it (`scripts/check-platform.sh` checks). The design, the full tool inventory and the decisions behind it are in `docs/research/mcp/`; this file is what exists now.

The protocol half is the `mcp` crate (`axcenity-mcp`): `rmcp` on a thread of its own, tool specs and their flat JSON schemas, argument checks and the one error shape. The tools are the editor's, in `editor/src/app/mcp.rs` and one file per tool in `app/mcp/`, so they work through the same `EditorApp` code the panels use. Each call is handed to the thread that owns the `EditorApp` as a `Job` and runs one at a time; a panic in a tool becomes an `internal` error instead of ending the server.

## Running it

```
axcenity-editor --mcp <project folder> [--profile lite|standard|full] [--mode read|edit|full]
```

`--project <folder>` works in place of the positional folder. If the folder isn't a project it says why on stderr and exits with 1. stdout carries only protocol messages; everything else goes to stderr. Closing stdin ends the server. Edits stay in memory until `project_save`: closing without saving loses them (the undo history goes with the process).

Both protocol eras are served: the `initialize` handshake (2025-11-25 and earlier) and the stateless 2026-07-28 one (`server/discover`, version in `_meta`).

### Connecting a client

Claude Code, from the project folder or anywhere:

```
claude mcp add axcenity -- axcenity-editor --mcp /path/to/project
```

Clients that read a JSON config (Cursor's `.cursor/mcp.json`, Claude Code's `.mcp.json`):

```json
{ "mcpServers": { "axcenity": { "command": "axcenity-editor", "args": ["--mcp", "/path/to/project"] } } }
```

Codex (`~/.codex/config.toml`, or `codex mcp add axcenity -- axcenity-editor --mcp /path/to/project`):

```toml
[mcp_servers.axcenity]
command = "axcenity-editor"
args = ["--mcp", "/path/to/project"]
```

## Profiles

Each tool belongs to a profile; a server lists the tools of its profile and the smaller ones. `standard` is the default. The test `every_tool_is_short_flat_and_in_budget` holds each profile's `tools/list` under its token budget (about lite 2000, standard 4500, full 6500), checks names and one-sentence summaries, and that no schema uses `oneOf`, `anyOf` or `$ref`.

## Naming objects and values

- An object is named by ID or path. An ID is the arena index, with `.generation` when that isn't 0 (`"17"`, `"17.2"`); every result shows IDs. A path is names from a service down, joined by `/` (`"Workspace/House/Door"`, `game/` in front is allowed; `"game"` is the root). A deleted object's ID is `stale_id`; a path two siblings share is `ambiguous` and lists their IDs; a wrong path suggests the closest names.
- Values are in `world.json` form, compact: `[x,y,z]` vectors, `[r,g,b]` colors 0–1, numbers to 3 decimals, enum names as strings, object references as quoted IDs (results add the target's class and name).

## Results and errors

Results are short text, one line per object, so small models can read them. A failed call is a tool result with `isError` and two lines:

```
error not_found: no class "PointLite"
fix: did you mean PointLight?
```

Codes: `bad_args`, `not_found`, `stale_id`, `ambiguous`, `unknown_property`, `bad_value`, `locked`, `not_allowed`, `needs_confirm`, `too_large`, `busy`, `unsupported`, `internal`. An argument the tool doesn't take, of the wrong type, or missing is `bad_args`, which names the tool's parameters. Only an unknown tool name is a protocol error.

## Tools

| Tool | Profile | What it does |
| --- | --- | --- |
| `project_info` | lite | The project's name, engine and toolkit versions, the open level and the others, object and script counts, the mode. |
| `scene_inspect` | lite | Without filters, the tree under `root` (default Workspace, `"game"` for everything) to `depth` (default 2); a run of siblings with the same class and name folds into one line (`×40`), and an unopened object says `(+N inside)`. With `class` (its subclasses too: `BasePart`, `Light`), `name` (contains, any case), `tag`, or `near` + `radius` (default 50), the matches with their paths. Cut at `limit` lines (default 60, at most 500) with a `cursor` for the rest. |
| `object_get` | lite | `id` or `ids` (at most 50). `detail`: `min` the header line, `std` (default) the properties that differ from the class defaults, `full` every property, attributes, tags and the first 20 children. `props` narrows it to those properties. |
| `docs` | lite | `class`: its superclasses, one-line description, properties with type, default and meaning, and events. `class` + `property`: that property in full, with the choices of an enum. `query`: a search of the script API. No arguments: every class and every `object_create` kind. Read from the same text as the script API reference and the Properties panel's descriptions. |
| `object_create` | lite | One object as Insert makes it (`tree::create`, the usual parent from `tree::insert_parent`, a name no sibling has). `kind` is an Insert name or class (`Block`, `Sphere`, `PointLight`, any case); the kinds whose Insert builds several objects or files (Character, controllers, Health, Inventory, health bar, splines, probes, MeshPart, Fracture, Page) are `unsupported` for now. `name`, `parent`, `position` (default: a part rests on the ground at the origin), `size`, `color`, `material` and `props` (other properties). |
| `object_set` | lite | The same change to `id`/`ids` (at most 2,000): `props`, `name`, `parent` (`tree::check_move`), `attrs` (`null` removes), `tags_add`, `tags_remove`. Returns the names changed. Locked objects (or ones in a locked Model) are `locked` unless `props` sets `Locked` false; services can't be renamed or moved. Over 50 objects asks first. |
| `object_delete` | lite | The editor's Delete (`tree::check_delete`; objects leave the tree). Up to 5 objects in all, with no script among them, go at once; more, or any script, return a preview (what goes, its scripts, the properties outside that will point at nothing) and a confirm token. Scripts' files stay on disk. |
| `history` | lite | `action`: `list` (default; the undo steps newest first, the next redos, saved or not), `undo` or `redo` with `steps` (default 1, at most 20). Undo and redo only take this client's own steps (`AI (<client>): …`); a step anyone else made stops them. |
| `project_save` | lite | The editor's Save (`EditorApp::save`: the world file and every unsaved script, graph and table; a "Save" version is kept). Asks first. |

Scripts, batches and the live editor connection come next (`docs/research/mcp/architecture.md`, steps 3.4 and 4.x).

## Edits and safety

- **Modes** (`--mode`, reported by `project_info`): `read` refuses every tool that changes anything (`not_allowed`, naming the flag that allows it); `edit` (default) runs edits and asks first for destructive ones; `full` runs everything without asking. Each tool has one safety class (read, add, modify, destructive), which sets its MCP annotations; the tool itself decides per call whether a call asks first (a 3-object delete doesn't, a 300-object one does).
- **One undo step per call.** `app/mcp/ops.rs::write` works like a plugin call: it copies the world, runs the change with the world's change log on, and records the objects the log names as one step (`History::record_edits`) labelled `AI (<client>): <what>`, where the client is its self-reported `clientInfo.name`. If the change fails part way (a bad value on the third object), the copy is put back, so a failed call changes nothing and leaves no step.
- **Asking first.** In `edit` mode a destructive call without `confirm` changes nothing and returns `needs_confirm`: `preview: …` and a fix line with a 6-character token. The same call again with `"confirm":"<token>"` runs it. A token (`axcenity_mcp::Confirms`) is bound to the tool, its arguments (all but `confirm`) and the history's revision, works once (a wrong try uses it up too), and expires after 2 minutes.
- **A version first.** The session's first write keeps the project files as a pinned version, `Before AI session (<client>)`, in Version History (`project::record_checkpoint`), so everything the session saved can be rolled back.
- **The log.** Every call adds a line to `.axcenity/mcp-log.jsonl` in the project (never exported): `time` (Unix seconds), `session`, `client`, `tool`, `class`, `args` (cut at 300 characters), `result` (`ok` or the error code) and `undo` (the step's label).
- Only what the editor lets you change: objects in the Workspace, ReplicatedStorage and Lighting (`tree::check_editable`), never services, Players or locked objects. No Lua, shell or network, and no files but what Save writes.

## Tests

- `crates/mcp/tests/protocol.rs`: a client in memory against the server, in both protocol eras.
- `crates/mcp/src/safety.rs`: modes, call keys and confirm tokens (once only, expiry).
- `editor/src/app/mcp/tests.rs`: each tool on copies of the examples, called as a client would: undo and redo, rollback of a failed write, confirm tokens, modes, saving and the log.
- `crates/editor/tests/mcp_stdio.rs`: the real binary over stdio; every stdout line is JSON-RPC, a write's undo label names the client, and it exits when stdin closes.
