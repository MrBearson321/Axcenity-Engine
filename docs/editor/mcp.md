# MCP server for AI clients

`axcenity-editor --mcp <project>` opens a project without a window and serves it to AI clients (Claude Code, Cursor, Codex, local models) over the Model Context Protocol, on stdin and stdout. Editor-only: the player and server never link it (`scripts/check-platform.sh` checks). The design, the full tool inventory and the decisions behind it are in `docs/research/mcp/`; this file is what exists now.

The protocol half is the `mcp` crate (`axcenity-mcp`): `rmcp` on a thread of its own, tool specs and their flat JSON schemas, argument checks and the one error shape. The tools are the editor's, in `editor/src/app/mcp.rs` and one file per tool in `app/mcp/`, so they work through the same `EditorApp` code the panels use. Each call is handed to the thread that owns the `EditorApp` as a `Job` and runs one at a time; a panic in a tool becomes an `internal` error instead of ending the server.

## Running it

```
axcenity-editor --mcp <project folder> [--profile lite|standard|full]
```

`--project <folder>` works in place of the positional folder. If the folder isn't a project it says why on stderr and exits with 1. stdout carries only protocol messages; everything else goes to stderr. Closing stdin ends the server.

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

## Tools (read-only so far)

| Tool | Profile | What it does |
| --- | --- | --- |
| `project_info` | lite | The project's name, engine and toolkit versions, the open level and the others, object and script counts, the mode. |
| `scene_inspect` | lite | Without filters, the tree under `root` (default Workspace, `"game"` for everything) to `depth` (default 2); a run of siblings with the same class and name folds into one line (`×40`), and an unopened object says `(+N inside)`. With `class` (its subclasses too: `BasePart`, `Light`), `name` (contains, any case), `tag`, or `near` + `radius` (default 50), the matches with their paths. Cut at `limit` lines (default 60, at most 500) with a `cursor` for the rest. |
| `object_get` | lite | `id` or `ids` (at most 50). `detail`: `min` the header line, `std` (default) the properties that differ from the class defaults, `full` every property, attributes, tags and the first 20 children. `props` narrows it to those properties. |
| `docs` | lite | `class`: its superclasses, one-line description, properties with type, default and meaning, and events. `class` + `property`: that property in full, with the choices of an enum. `query`: a search of the script API. Read from the same text as the script API reference and the Properties panel's descriptions. |

Writes (with undo, a log and confirmation for destructive steps), scripts and the live editor connection come next (`docs/research/mcp/architecture.md`, steps 3.3 and 3.4).

## Tests

- `crates/mcp/tests/protocol.rs`: a client in memory against the server, in both protocol eras.
- `editor/src/app/mcp/tests.rs`: each tool on copies of the examples, called as a client would.
- `crates/editor/tests/mcp_stdio.rs`: the real binary over stdio; every stdout line is JSON-RPC and it exits when stdin closes.
