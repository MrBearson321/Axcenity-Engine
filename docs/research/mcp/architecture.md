# Axcenity MCP toolkit: architecture proposal (Phase 2)

Status: **proposal, awaiting Paul's approval** (decisions in [README.md](README.md#decisions-for-paul)). Nothing here is implemented yet.

Inputs: the engine audit ([engine-capability-map.md](engine-capability-map.md)), the proposed tools ([tool-inventory.md](tool-inventory.md)) and the MCP spec and client research ([client-compatibility.md](client-compatibility.md)).

## 1. Recommendation in one page

- **One implementation, two hosts.** The MCP tools live inside the editor, on the same `EditorApp` operations, undo history and validation the editor's own buttons use. They run either **live** (inside the editor the user has open, so they watch the AI work and can undo it) or **headless** (the same editor code with no window, for cloud agents, CI-less local runs and machines without a GPU).
- **Clients always launch the same command**: `axcenity-editor --mcp <project>` over **stdio**. If the editor already has that project open with AI connections allowed, the process just relays to it; otherwise it hosts the project headless. Client config never changes between the two cases.
- **The official Rust SDK (`rmcp` 3.x) speaks the protocol**, both the current stateless revision (2026-07-28) and the handshake revision most clients still use (2025-11-25). A small crate of ours (`crates/mcp`, no engine dependencies) holds the tool registry, schemas, errors, safety classes, confirm tokens, paging and the token-budget tests. The editor owns the tools.
- **~20 tools, not hundreds**, in three profiles (`lite` 11, `standard` 21, `full` 28), sharing one parameter vocabulary. Compact text output with non-default properties only; change cursors instead of re-reading scenes.
- **Safety from the engine's own model**: every write is one labelled undo step; destructive calls return a preview and need a `confirm` token; a version checkpoint before each AI session; an audit log; no arbitrary Lua, no files outside the project, no network.
- **`batch`** runs several writes as one all-or-nothing transaction and one undo step, by the same copy-run-diff pattern editor plugins already use.

## 2. Goals and non-goals

Goals are the brief's §1. Non-goals for this design:

- Remote/internet access to the editor (only local: stdio and loopback).
- A second scripting interface. Agents write Lua *files* through `script_write`; they don't execute Lua in the editor.
- Mirroring every editor panel. Visual editors (block scripts, material/UI/behavior graphs, Model Studio selections) stay UI-first until there's a reason (tool-inventory "Deferred").
- Provider-specific code. Nothing in the engine knows which model or client is connected beyond the self-reported `clientInfo.name` used for labels and the log.

## 3. Hosting

### 3.1 Options considered

| Option | For | Against | Verdict |
|---|---|---|---|
| **A. In-process in the editor only** (server thread inside the GUI editor; client connects over HTTP) | Live, the user sees changes; one state | Needs the editor open; most clients launch servers as stdio processes; HTTP server dependency; no use in the cloud | Part of the answer |
| **B. Separate local service** (own process loading the project with engine crates, no editor) | Independent of the GUI | Reimplements Insert rules, undo, script-file rules, asset fixes: all live in the `editor` binary today. Two writers on one project. Inconsistent results with the editor | Rejected |
| **C. Hybrid: editor binary in `--mcp` mode** (stdio; relays to a live editor if one has the project, else headless `EditorApp`) | Same code paths in both cases; stdio works with every client; testable headless in the cloud; no async runtime | A relay step and a lock file to design | **Recommended** |

Option B is what "directly manipulating internal data structures" would look like; C uses `EditorApp` as the stable engine API, which is what the brief asks for.

### 3.2 Process layout

```
AI client ──stdio──▶ axcenity-editor --mcp <project>
                        │
                        ├─ editor running with <project> and AI allowed?
                        │     yes ──loopback TCP (127.0.0.1, token)──▶ live editor: McpHost thread ─▶ main thread
                        │     no  ──▶ headless EditorApp in this process: McpHost ─▶ same tools
```

- **Live host**: when the user turns on *Allow AI connections* (editor setting, per project), the editor opens a loopback listener on a random port and writes `.axcenity/mcp-endpoint.json` (port, random token, editor PID). The `--mcp` process reads it, connects, sends the token, and pipes bytes both ways. Several clients can connect at once; each is a separate session with its own client name.
- **Headless host**: `EditorApp::new(Some(project))` with no window, exactly as the editor's tests run it. It writes `.axcenity/mcp-headless.lock` (PID, client). It never saves unless `project_save` is called.
- **Two writers**: a headless host checks for a live endpoint before each write; if the editor has since opened the project, it refuses writes ("The editor has this project open; restart the MCP server to connect to it"). The editor, opening a project with a headless lock whose PID is alive, says "An AI session is editing this project without the editor" and offers to open anyway. No silent last-write-wins.

### 3.3 Threading in the live editor

All editor state is on the egui main thread. The host thread parses and schema-checks JSON-RPC, then sends `(Request, reply_sender)` over a channel and calls `egui::Context::request_repaint()` so an idle editor wakes. At the start of `EditorApp::update` the main thread drains the queue within a time budget (4 ms per frame, then the rest waits a frame) and replies. Long work (erosion, tests, Play for N seconds) starts the editor's existing background jobs. A call answers within 45 s (Codex times tool calls out at 60 s, Claude Desktop at 240 s); work that isn't done by then returns a `job` handle and the agent calls again with it, which every client can do. Progress notifications are sent where the client supports them. Writes during Play or a viewport drag are refused with `busy`, as `EditorApp::act` already ignores them.

Read requests also go through the main thread; that keeps one consistent view and no locks around `World`. If profiling shows idle-frame cost, the drain is skipped when the queue is empty (one atomic load).

### 3.4 Transport and dependencies

- **v1: stdio only**, newline-delimited JSON-RPC as the spec requires and recommends for local servers. Every target client supports stdio except Open WebUI and the OpenAI Responses API, which only take an HTTP URL ([client-compatibility.md §3.1](client-compatibility.md#31-transports-and-protocol-features)). The relay's loopback leg pipes the same bytes over TCP: no HTTP.
- **Later, only if a user needs Open WebUI or the Responses API: Streamable HTTP** on 127.0.0.1 from the live editor, with Origin validation turned on explicitly (rmcp leaves `allowed_origins` empty, which disables it) and a bearer token. Never the deprecated HTTP+SSE transport.
- **Library** (decision D2): the spec now has two live eras. The 2026-07-28 revision is stateless (no `initialize`, a required `server/discover`, `_meta` on every request, `resultType` on every result), while most deployed clients still speak 2025-11-25 with the `initialize` handshake. `rmcp` 3.9 (Apache-2.0, MSRV 1.88, Tier-1 official SDK, used by Codex) answers both. Hand-writing both eras is 300–600 lines plus following spec churn ourselves. Recommended: **`rmcp` with only the `server` and `transport-io` features**, its runtime on the MCP host thread (a current-thread `tokio` runtime) bridged to the main thread by the queue in §3.3. It adds `tokio` and about 68 crates to the **editor only**; the player and server never link it, and the platform check can assert that. We implement `ServerHandler` by hand over our registry rather than rmcp's `#[tool]` macros, so schemas stay hand-tuned and small (§5).
- **Process lifetime**: clients kill stdio servers quickly (Claude Code on Windows: 300 ms after closing stdin). The headless host never leaves a half-written file: every write goes to a temporary file and is renamed into place.

## 4. Layers and registration

### 4.1 Crates and modules

| Layer | Where | Knows about | Tested by |
|---|---|---|---|
| Protocol | `crates/mcp` (`axcenity-mcp`, library) | JSON-RPC, MCP messages, `ToolSpec`, `Class`, `Profile`, `Detail`, `ToolError`, confirm tokens, cursors, token estimates | Pure unit tests, golden transcripts |
| Host | `editor/src/mcp/host.rs`, `relay.rs` | stdio/TCP, sessions, live/headless, lock files, main-thread queue | Two-host tests with an in-process `EditorApp` |
| Tools | `editor/src/mcp/tools/<tool>.rs` | Parameter parsing, output formatting, which ops to call | One test file per tool on copied examples (`copy_of_example`) |
| Engine ops | `editor/src/mcp/ops/` | `EditorApp`, `History`, `project`, `terrain`, `anim`, … | Shared by every tool; the only place that changes the world |

**Engine ops** are typed Rust functions such as `ops::create(app, Insertable, parent, name, props) -> Result<Created, OpError>` and `ops::set(app, ids, Fields) -> Result<Changed, OpError>`. They call what the UI calls (`insert_named`, `apply_many`, `act`, `History::record_*`, `project::write_script`, …), so a part made by AI is the same as one made by Insert, and an undo step looks the same. Where the UI path assumes a pointer (e.g. `insert` uses the camera focus), the op takes the value explicitly and calls the lower function the UI also uses (`insert_named`). Ops are the "stable engine API" of brief §3; any editor refactor that breaks one breaks its tests.

High-level tools (`env_set`, `object_create` with `material`, `terrain_edit generate`) and low-level ones (`object_set`) both call ops. There is no second path, so the two layers can't disagree (brief §6).

### 4.2 Registering a tool

```rust
// editor/src/mcp/tools/object_set.rs
pub const SPEC: ToolSpec = ToolSpec {
    name: "object_set",
    summary: "Change properties, name, parent, attributes or tags of objects.",
    profile: Profile::Lite,
    class: Class::Modify,          // the default; `classify` can raise it per call
    params: &[ID_OR_IDS, PROPS, NAME, PARENT, ATTRS, TAGS_ADD, TAGS_REMOVE],
    example: r#"{"ids":["41"],"props":{"Brightness":3}}"#,
};
pub fn classify(ctx: &Ctx, args: &Args) -> Class { … }   // e.g. > 50 objects → Destructive
pub fn call(ctx: &mut Ctx, args: Args) -> ToolResult { … }
```

`tools/mod.rs` lists every tool in one array. Shared parameters (`ID_OR_IDS`, `PROPS`, `DETAIL`, …) are constants, so a name and its description are written once. A test walks the array and checks each tool has a summary under 120 characters, an example that parses and runs on `hello-part`, a profile, a docs entry, and that the profile budgets hold.

**Extending**: a new engine system adds ops, one tool file and one array line. Plugin-registered tools (Lua plugins exposing their own MCP tools) are possible later through the plugin API, but not in v1 (no infrastructure before it's needed).

### 4.3 Versioning

- MCP protocol version: both eras through rmcp (2026-07-28 `server/discover` and `_meta`; 2025-11-25 and older `initialize`). Tool lists don't vary per connection (a 2026-07-28 rule): the profile is fixed per server process by `--profile`, and state that spans calls (change cursors, confirm tokens) is passed as explicit tool arguments, which is what the stateless revision asks for.
- Toolkit version: semver in `serverInfo.version` and `project_info`. Tool names and parameter meanings are stable within a major version. A new optional parameter is minor; renaming/removing is major and keeps an alias for one major version.
- `docs/mcp/changelog.md` records every tool change.

## 5. Tool conventions

- **Names**: `noun_verb` in snake_case (`object_set`), ≤ 20 characters, ASCII. Prefix groups sort together in client UIs; `mcp__axcenity__object_duplicate` stays far under Claude Code's 128 and Gemini's 63-character limits.
- **Annotations on every tool**: `title`, `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint: false`. Claude, VS Code and Codex (`writes` mode) skip confirmation for read-only tools and always ask for destructive ones, so accurate annotations are what make the toolkit pleasant to use without lowering safety (the server still enforces §7).
- **Descriptions**: one sentence plus at most one example call. Value formats, class lists and workflows live in `docs` and resources, not in descriptions.
- **Schemas**: a plain `type:"object"` root; at most one level of nesting (`props`, `ops[].args`); `enum` for every closed set under ~12 values; plain strings validated by the server for big sets (Insert kinds, class names), with close matches in the error. No top-level `oneOf`/`anyOf`/`allOf` (Claude Code flattens them, OpenAI strict mode rejects them), no `$ref`, no reliance on `additionalProperties` (Gemini strips it). Each input schema under 2 KB (Codex compacts schemas over 5 KB). Defaults stated in the schema.
- **Values**: the `world.json` encoding (`project::world_file`'s codec, made public), so docs, files and MCP agree: `[x,y,z]`, `[r,g,b]` 0–1, enum names, project paths.
- **Results**: compact text lines (examples in the inventory). Mutations return what changed and the undo label, not the object. Every result fits a cap (≈ 4,000 tokens by default); beyond it the result says how much was cut and returns a `cursor`.
- **Structured output**: `structuredContent` with `outputSchema` only for the few results a program would parse (`project_info`, `history changes`), because the spec asks for the same JSON in a text block too for older clients, which doubles tokens elsewhere (decision D4).
- **Errors** are tool results with `isError: true`, never raw panics or engine strings, in one shape:

```
error not_found: no object "Workspace/Hous"
fix: did you mean "Workspace/House"? Use scene_inspect to list objects.
```

| Code | When | Always includes |
|---|---|---|
| `bad_args` | missing/invalid parameter | which parameter, an example of a valid value |
| `not_found` / `stale_id` | no such object/file; ID of a deleted object | close matches; `stale_id` says it was deleted and by which step |
| `ambiguous` | path matches several objects | the candidates with IDs |
| `unknown_property` | property not on class | close names, `docs {class}` hint |
| `bad_value` | wrong type or out of range | expected format and range |
| `locked` | object is Locked | how to unlock |
| `not_allowed` | permission mode forbids it | the mode and how the user changes it |
| `needs_confirm` | D call without token | the preview and token |
| `too_large` | over a cap | the cap and how to page or split |
| `busy` | Play running, drag in progress, job running | when to retry |
| `unsupported` | missing engine capability | the gap ID (G8, …) |

## 6. Context and token efficiency

### 6.1 References

- **IDs** are the arena index (plus generation when non-zero: `"17"`, `"17.2"`), stable for the editor session, through delete and undo (deleting detaches). Short and cheap.
- **Paths** (`Workspace/House/Door`) work everywhere an ID does and survive reopening; duplicates resolve to the first in tree order only if unique, else `ambiguous`.
- Every result names objects as `ID Class "Name"`, so the model can reuse IDs instead of re-searching.

### 6.2 Smaller results

- `object_get` std = **non-default properties only** (a Part has 26 properties; a typical one differs in 4–6). Numbers to 3 decimals, no trailing zeros.
- One-line-per-object lists; repeated siblings folded (`Fence ×40`); `limit` + `cursor` on every list.
- `detail` levels (`min`/`std`/`full`) with per-tool defaults: lists `min`, single objects `std`.
- `props` filters on reads; mutations echo only changed names.

### 6.3 Changes instead of re-reads

The host keeps the edit world's change log on (`World::record_changes`) and drains it every frame into a bounded ring (like `WORKSPACE_MOVES_KEPT`), numbering changes. Every result carries `cursor`. Reads accept `since: cursor` and return only what changed; `history {action:"changes"}` summarises creates, removals and changed property names, including the **user's** edits in live mode ("the user moved 41 and deleted 44"). If a cursor fell out of the ring the answer says so and the agent re-reads.

A separate snapshot system was considered and dropped: cursors plus `Instance::revision` cover "what changed", and version checkpoints (`project::versions`) cover "go back to before the AI started". No new storage.

### 6.4 Definitions budget and measuring it

- Profiles hide tools a model won't use. Vendors report tool-selection accuracy dropping past 30–50 tools and OpenAI suggests fewer than 20 up front; `standard` has 21 and `lite` 11. Claude Code, Cursor and OpenAI now load tool definitions on demand, so there `standard` costs less than its full budget.
- `tools/list` returns tools in a fixed order (the spec recommends it; it keeps client prompt caches warm).
- Server `instructions` (sent by `initialize` / `server/discover`) ≤ 150 tokens: value format, the ID/path rule, "use `docs` for classes", "writes are undoable; deletes ask first".
- A test serializes `tools/list` per profile and fails if the token estimate exceeds the budget (lite 2,000 / standard 4,500 / full 6,500). The estimate is calibrated once against real tokenizers (Claude and `o200k`) and recorded in `docs/mcp/benchmarks.md`; the test itself uses the calibrated characters-per-token ratio so it needs no tokenizer dependency.

## 7. Safety and permissions

### 7.1 Classes and per-call classification

| Class | Examples | Default behaviour (mode `edit`) | MCP annotation |
|---|---|---|---|
| R | inspect, find, docs, check, run tests | runs | `readOnlyHint: true` |
| A | create object, material, script, import | runs, one undo step | `destructiveHint: false` |
| M | set properties, edit script, terrain brush, env | runs, one undo step | `destructiveHint: false`, `idempotentHint` where true |
| D | delete over threshold or with scripts, replace landscape, save, edit a material used by > 50 parts, project settings removals, version restore | **preview + `confirm`** | `destructiveHint: true` |

Tools classify **each call** (`classify`), so a 3-object delete is M and a 300-object one is D. Annotations are hints for clients' own approval UIs; the server enforces regardless.

### 7.2 Permission modes

Set by the user per project (editor setting; headless: `--mode`), reported in `project_info`:

| Mode | R | A/M | D |
|---|---|---|---|
| `read` | yes | refused (`not_allowed`) | refused |
| `edit` (default) | yes | yes | preview + confirm |
| `full` | yes | yes | yes, still undoable and logged |

### 7.3 Confirmation

A D call without `confirm` changes nothing and returns a preview (what goes or changes, counts, scripts, references that will dangle) and a token. The token is bound to a hash of the arguments and the world's revision, single-use, and expires after 2 minutes, so a model can't reuse it for a different or stale request. Works in every client because it's ordinary tool calls. In the live editor the preview can also show as an Allow/Deny toast; MCP elicitation is used only where the client supports it (optional).

### 7.4 Undo, revert, audit

- Every write is one undo step labelled `AI (<client>): <what>` (`AI (Claude Code): Delete House`), recorded with the existing `History` recorders. The user undoes it like any other step.
- First write of each session records a version checkpoint `Before AI session (<client>)` (`project::record_checkpoint`), so a whole session can be rolled back from Version History even after saves.
- `.axcenity/mcp-log.jsonl`: one line per call (time, session, client, tool, class, arguments summary, result, undo label). `.axcenity` is already excluded from exports.
- `history` lets the agent list and undo its own top steps; it can't undo the user's.

### 7.5 Limits and boundaries

- Paths: the plugin rules (`plugins/src/files.rs`): project-relative, `/`, no `..`, no absolute, no links out. Imports read only from folders the user allows in settings.
- No Lua execution, no shell, no network (Toolbox downloads later go through its backend), no editor/user settings writes.
- Locked objects are refused. Services can't be deleted or reparented (tree rules in `tree.rs`).
- Caps: request 1 MB, `batch` 100 ops / 2,000 objects, results ≈ 4,000 tokens before paging, Play ≤ 60 s, one running job per session.
- Auth: stdio is the user's own process. The loopback relay binds 127.0.0.1 only and requires the token from the endpoint file (user-only permissions). `clientInfo.name` is self-reported: used for labels, not trust.

## 8. Batch and transactions

1. **Check** every op before running any: tool exists and may be batched, arguments valid, `$name` references refer to earlier ops, permission mode allows each op's class, D ops covered by the batch's `confirm`.
2. **Run** in order on the edit world with the change log on, after copying the world (plugin pattern; 11.5 ms for 10k Parts, PR #87). File writes (scripts, materials) keep their before-text.
3. **On failure**: put the world copy back and restore the before-texts; report `op 3 (object_create) failed: unknown_property "Colour" — did you mean Color`; nothing is applied.
4. **On success**: one undo step from the diff (`History::record_edits`) plus the file changes, labelled `AI (<client>): Batch (n)` or the batch's `label`.

Terrain generation, Play, tests and saves are not batchable (large state or side effects outside the world).

## 9. High-level versus low-level tools

Estimated for five reference workflows; Phase 7 replaces the estimates with measurements (`docs/mcp/benchmarks.md`).

| Workflow | Low-level only (calls) | With high-level params/tools (calls) | Saved |
|---|---|---|---|
| Textured block (create, material, assign, check) | 4 (`object_create`, `material_edit create`, `material_edit assign`, `project_check`) | 1 (`object_create` with `material`) | 3 calls, ≈ 60% tokens |
| Small hut (model, floor, 4 walls, roof, light) | 8 | 1 `batch` | 7 round trips; tokens similar (same args) |
| Sunset scene look (time, atmosphere, clouds, fog, bloom) | 6–8 (`object_create` ×3, `object_set` ×3–5) | 1 `env_set` | most tokens and the need to know effect classes |
| Door script + try it | 3 (`script_write`, `game_run play`, `script_read` on error) | 3 | none: already minimal |
| Fix a missing texture | 3 (`project_check`, `asset_find`, `object_set`) | 3 | none |

Conclusions: high-level helps where the low-level route needs **engine knowledge** (which effect classes, which properties) or **many round trips** for one intent. Elsewhere the low-level tools are already one call. So the high-level layer is deliberately small: convenience parameters on `object_create`, `env_set`, `terrain_edit generate`, `anim_edit animate`, and `batch`. No `build_scene` that duplicates `batch`.

## 10. Small local models

- `lite` profile: 11 tools, one purpose each, flat parameters, enums spelled out.
- Defaults that make the common call short: `parent` = Workspace, `detail` per tool, sizes from Insert defaults.
- Errors always say what to do next, with valid values and close matches (§5).
- Results start with the answer (`created 233 Part "Wall"`), not JSON to parse.
- Two prompts (`axcenity_build`, `axcenity_fix`) give a fixed recipe instead of relying on the model to plan.
- Evaluated with at least two local models in Phase 5/7 on Paul's PC (via the coordinator), measuring task success and calls per task against the same tasks with a cloud model.

## 11. Testing (TDD) and measurements

| Area | Tests |
|---|---|
| Protocol | Golden transcripts for both eras through rmcp: 2025-11-25 `initialize` + `tools/list` + `tools/call`, 2026-07-28 `server/discover` + `_meta` + unsupported-version error; cancellation, paging, unknown tool, malformed arguments → `isError` results |
| Registry | every tool valid per §4.2; profile budgets; names/limits |
| Each tool | success, invalid args, unknown IDs/paths, stale IDs, locked objects, every error code it can return, the undo step it records (undo restores exactly) — on copied examples with headless `EditorApp` |
| Safety | mode enforcement, per-call classification thresholds, confirm token binding/expiry/single use, path rules, caps |
| Batch | ordering, `$name` references, rollback on failure at each position, file before-images restored, one undo step |
| Hosts | headless and live (in-process `EditorApp` with the queue), relay byte-piping, two clients at once, client disconnect mid-request, editor gone → relay error, lock-file conflicts |
| Large scenes | S10k (`--bench` scene): `scene_inspect` paging, `object_find` by class/tag, `history changes` latency and response size |
| Clients | MCP Inspector smoke test; Claude Code in the cloud; Cursor, Codex, a local model on Paul's PC (coordinator) |

Measured and recorded in `docs/mcp/benchmarks.md`: definition tokens per profile, average response tokens per tool, calls per reference workflow (high vs low), request latency (headless and live), per-frame overhead in the live editor when idle and busy, memory of the change ring, S10k timings.

## 12. Roadmap

Each step is a PR with tests, merged before the next starts depending on it.

| Step | Contents | Depends on | Notes |
|---|---|---|---|
| 3.1 | `crates/mcp`: registry, `ToolSpec`, errors, classes, confirm tokens, cursors, budget test; `rmcp` pinned in the workspace (licence check, `cargo hakari generate`) | approval | Cloud |
| 3.2 | `editor --mcp` headless host on stdio; `project_info`, `scene_inspect`, `object_get`, `docs` (read-only) | 3.1 | First usable server: Claude Code can read a project |
| 3.3 | Ops layer + `object_create/set/delete`, `history`, undo labels, audit log, session checkpoint, permission modes | 3.2 | |
| 3.4 | Live host: editor setting, loopback listener, relay, lock files, main-thread queue | 3.3 | GUI check on Paul's PC |
| 4.1 | Scripts and checks: `script_read/write`, `project_check`, `game_run` | 3.3 | Completes `lite` |
| 4.2 | `batch`, `object_duplicate`, `material_edit`, `env_set` | 3.3 | |
| 4.3 | `terrain_edit`, `anim_edit`, `asset_find`, `asset_import`, `project_save` | 3.3 | Completes `standard` |
| 4.4 | `full` profile tools | 4.3 | |
| 5 | Change cursors everywhere, folding, prompts, resource mirrors; small-model pass | 4.x | Local models on Paul's PC |
| 6 | `docs/mcp/` user docs, client configs, diagnostics (`axcenity-editor --mcp --check`) | 3.4 | |
| 7 | Benchmarks, confusing-tool review, schema simplification | all | |
| later | `view_capture`, Toolbox tools (after #90 lands), Streamable HTTP if a client needs it, `data_edit`, mesh ops | as listed | |

Coordination: G2 (scripts inside objects, #91) changes where scripts live; `script_write` follows whatever lands. G4 (luau-lsp) can later feed `project_check` with type errors. The Toolbox thread owns `crates/toolbox`; MCP only wraps its API. M12 (egui 0.36) touches `EditorApp::update`; the live host's hook is one call there, so the order doesn't matter much, but step 3.4 should land after M12 to avoid a conflict.

## 13. Simplifications taken

- No separate service process, no IPC protocol of our own: the relay pipes MCP bytes unchanged.
- No HTTP in v1; the async runtime stays on the MCP thread of the editor binary.
- No snapshot system: change log cursors + version checkpoints.
- No duplicate high-level tool family: convenience parameters + `batch` + three genuinely multi-step tools.
- No per-client code: one server, client differences documented in config examples.
- Resources mirror tools; nothing is resource-only.
- Plugin-defined MCP tools, mesh editing, graphs and screenshots wait until the core is used.
