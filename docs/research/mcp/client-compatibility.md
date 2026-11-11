# MCP client compatibility research (for an Axcenity editor MCP server)

Checked on 2026-11-11 against primary sources: the MCP spec, the official SDK repos, and docs from client
vendors. Each claim carries a source tag such as `[S2]`, and the URLs are in **Sources** at the end. Where a
vendor doesn't document something, the cell says "not documented" rather than guessing. "n/d" means not documented.

---

## 1. MCP specification: where it stands now

### 1.1 Revisions

| Revision | Status | Notes |
|---|---|---|
| **2026-07-28** | **Current** | Stateless redesign. No `initialize` handshake [S1][S2] |
| 2025-11-25 | Final | Last "legacy" revision, the newest one with the `initialize` handshake [S3][S4] |
| 2025-06-18, 2025-03-26, 2024-11-05 | Final | Still supported by rmcp [R5] |

The spec now uses **Modern** for 2026-07-28 and later, **Legacy** for 2025-11-25 and earlier, and **Dual-era**
for an implementation that speaks both [S4].

### 1.2 What changed in 2026-07-28 that matters to a server [S2]

| Area | Change |
|---|---|
| Handshake | `initialize` and `notifications/initialized` are **removed**. Every request carries `_meta` with `io.modelcontextprotocol/protocolVersion` and `io.modelcontextprotocol/clientCapabilities` (both required), plus `clientInfo` (SHOULD) [S2][S9] |
| `server/discover` | New. Servers **MUST** implement it. It returns `supportedVersions`, `capabilities`, `instructions`, and `serverInfo` in `_meta` [S13] |
| Version errors | `UnsupportedProtocolVersionError` uses code `-32022` and returns a `supported` list [S4] |
| Results | Every result needs `resultType` (`"complete"` or `"input_required"`) [S2] |
| Sessions | `Mcp-Session-Id` is removed. `tools/list`, `resources/list` and `prompts/list` MUST NOT vary per connection. Cross-call state uses explicit handles passed as tool arguments [S2][S8] |
| Server→client requests | Replaced by **MRTR**: the server returns `InputRequiredResult` with `inputRequests` (elicitation, sampling or roots), and the client retries the call with `inputResponses` [S2][S8] |
| Subscriptions | `resources/subscribe`/`unsubscribe` and the HTTP GET stream are replaced by **`subscriptions/listen`**. The client opts in to `toolsListChanged`, `promptsListChanged`, `resourcesListChanged` or `resourceSubscriptions[]`, and the server must acknowledge first [S2][S12] |
| Removed | `ping`, `logging/setLevel`, `notifications/roots/list_changed`. The log level is now set per request in `_meta` [S2] |
| Deprecated (earliest removal is the first revision on or after 2027-07-28) | **Roots, Sampling, Logging**, and Dynamic Client Registration. HTTP+SSE stays deprecated (since 2025-03-26) [S10] |
| Tasks | Moved out of core into the `io.modelcontextprotocol/tasks` extension [S2] |
| Caching | List and read results carry `ttlMs` and `cacheScope`. `tools/list` SHOULD return tools in deterministic order, which helps prompt caching [S2][S8] |
| Schemas | `inputSchema` and `outputSchema` may use any JSON Schema 2020-12 keyword, and `structuredContent` may be any JSON value. In 2025-11-25, `inputSchema` was `{type:"object", properties, required}` [S2][S9][S16] |
| Errors | "Resource not found" moves from `-32002` to `-32602`. MCP reserves `-32020..-32099` [S2] |
| HTTP | `Mcp-Method` and `Mcp-Name` headers are required. `x-mcp-header` mirrors tool parameters into headers [S2][S7] |

### 1.3 What 2025-11-25 added (still what most deployed clients speak) [S3]

Icons on tools, resources and prompts. Tool-name guidance (SEP-986). URL-mode elicitation and enum
improvements. Tools inside sampling. Experimental tasks. JSON Schema 2020-12 became the default dialect.
Input-validation failures are returned as **tool execution errors** (`isError: true`) rather than protocol
errors. Invalid `Origin` must get HTTP 403.

### 1.4 Server features: what's in the spec

| Feature | 2026-07-28 status | Notes |
|---|---|---|
| Tools + `outputSchema` / `structuredContent` | Active | If a tool declares `outputSchema`, its results MUST conform. It SHOULD also return the JSON as a text block for backward compatibility [S8] |
| Tool annotations | Active | `title`, `readOnlyHint` (default false), `destructiveHint` (default true), `idempotentHint` (default false), `openWorldHint` (default true). Destructive and idempotent are meaningful only when not read-only. Clients MUST treat annotations from untrusted servers as untrusted [S9][S8] |
| Tool names | Guidance only | SHOULD be 1–128 characters from `[A-Za-z0-9_.-]`, case-sensitive [S8] |
| Resources, resource templates | Active | `resources/list`, `resources/read`, `resources/templates/list` [S9] |
| Resource subscriptions + list_changed | Active (new mechanism) | Delivered through `subscriptions/listen` [S12] |
| Prompts | Active | `prompts/list`, `prompts/get` [S9] |
| Completions | Active | `completion/complete` [S9] |
| Elicitation | Active (via MRTR) | Form and URL modes [S2][S8] |
| Sampling, Roots, Logging | **Deprecated** | Pass paths as tool parameters or config, and log to stderr [S10] |
| Pagination | Active | `cursor` / `nextCursor` on list results [S8] |
| Progress, cancellation | Active | stdio cancels with `notifications/cancelled`. Streamable HTTP cancels by closing the response stream [S5][S6][S7] |
| Authorization (OAuth 2.1) | HTTP only | Client ID Metadata Documents are preferred, and DCR is deprecated [S10] |

### 1.5 Transports

| Transport | Status | Server-relevant rules |
|---|---|---|
| **stdio** | Standard | Newline-delimited JSON-RPC with no embedded newlines. Nothing but MCP messages may go to stdout. stderr may carry any log output. The server SHOULD exit promptly on stdin EOF. On Windows, clients may use `TerminateProcess` or Job Objects. Legacy-or-modern detection is by a `server/discover` probe [S6] |
| **Streamable HTTP** | Standard | A single POST endpoint that replies with JSON or a per-request SSE stream. **No GET stream and no sessions in 2026-07-28.** A modern-only server answers GET/DELETE with 405 [S7] |
| HTTP+SSE (2024-11-05) | **Deprecated** | "New implementations SHOULD NOT adopt it" [S7][S10] |

**Security guidance for a local HTTP server** [S7][S11]:
- Servers **MUST** validate `Origin` on every incoming connection, to block DNS rebinding. If `Origin` is present and invalid, respond with **HTTP 403**.
- When running locally, servers **SHOULD** bind to 127.0.0.1, not 0.0.0.0.
- Servers **SHOULD** authenticate every connection.
- For servers meant to run locally, the spec recommends stdio, because it limits access to the launching client. An HTTP server should require an authorization token, or use a Unix socket or other access-restricted IPC [S11].

---

## 2. Rust SDKs

### 2.1 `rmcp`, the official SDK (modelcontextprotocol/rust-sdk)

| Item | Value |
|---|---|
| Latest | **3.9.0**, released 2026-11-05. Releases come about every 1–2 weeks; 3.0.0 shipped 2026-07-28 [R1][R4] |
| License | **Apache-2.0**, matching Axcenity [R3] |
| MSRV / edition | `rust-version = "1.88"`, `edition = "2024"`. Compatible with a pinned 1.97 toolchain [R3] |
| SDK tier | **Tier 1**, alongside TypeScript, Python, C#, Go and Ruby [R7] |
| Spec revisions | `LATEST = 2026-07-28`. Also supports 2025-11-25, 2025-06-18 and 2025-03-26. The server answers a legacy `initialize` with its newest handshake version, so it is **dual-era** out of the box [R5][R2] |
| Runtime | **tokio is a mandatory dependency** (features `sync`, `macros`, `rt`, `time`) [R3] |
| Default features | `server`, `macros`, `base64` [R3] |
| stdio server | Feature `transport-io`, used as `Service.serve(rmcp::transport::stdio())` [R2][R3] |
| Streamable HTTP server | Feature `transport-streamable-http-server`, which provides `StreamableHttpService` as a tower service (mount it in axum, for example) [R2][R3] |
| Macros | `#[tool_router(server_handler)]` on an impl block, plus `#[tool(description = "…")]` methods taking `Parameters<T>`, where `T: Deserialize + schemars::JsonSchema` (schemars 1.x). It generates `ServerHandler` for a tools-only server. `#[prompt]` macros also exist [R2][R3] |
| HTTP hardening | `allowed_hosts` defaults to loopback (`localhost`, `127.0.0.1`, `::1`). **`allowed_origins` defaults to empty, which DISABLES Origin validation**, so you must set it or call `enforce_origin_validation` [R6] |
| stdio framing | The default maximum line length is 16 MiB [R9] |
| Other users | OpenAI Codex builds its MCP client and server on `rmcp` [C-CXS] |

**Dependency weight.** I measured this with `cargo tree -e normal` in a scratch crate on this machine
(cargo 1.97.0, 2026-11-11).
- `rmcp 3.9.0` with `server, macros, transport-io, schemars`, plus `tokio`, pulls in **68 unique crates**, the root included. The main ones are tokio, serde/serde_json, schemars and derive, futures, tracing, chrono, uuid, sha2, and syn/darling for the proc macros.
- Adding `transport-streamable-http-server` and `axum 0.8` raises that to **97 crates**. A cold debug build of that variant took about 30 s wall time here.

### 2.2 Other Rust options (none are official)

| Crate | Latest | License | MSRV | Notes |
|---|---|---|---|---|
| `rust-mcp-sdk` | 2.0.0 (2026-08-27) | MIT | 1.80 | rust-mcp-stack [R8] |
| `pmcp` | 2.23.0 (2026-10-28) | MIT | 1.91 | paiml [R8] |
| `tower-mcp` | 0.26.0 (2026-11-10) | MIT OR Apache-2.0 | 1.90 | Tower-native [R8] |

All three have far fewer downloads than rmcp (47.6M) [R1][R8]. None is Tier 1.

### 2.3 Hand-rolling JSON-RPC over stdio

It's feasible, but a server now has to handle **two eras**:

| Era | Minimum the server must implement |
|---|---|
| Legacy (2025-11-25 and earlier) | `initialize`: reply with the same `protocolVersion` if supported, otherwise the latest you support, plus `capabilities` and `serverInfo` [S14]. Accept `notifications/initialized` [S14]. Answer `ping` with `{}` (MUST) [S15]. Implement `tools/list` and `tools/call` [S8] |
| Modern (2026-07-28) | `server/discover` (MUST) [S13]. Read `_meta` on each request and return `UnsupportedProtocolVersionError -32022` for unknown versions [S4]. Every result includes `resultType: "complete"` [S2]. Implement `tools/list` and `tools/call` [S8], and honor `notifications/cancelled` [S6] |

Both eras need the stdio rules above [S6], `isError` tool errors [S8], and JSON Schema 2020-12 tool schemas [S3].

Assessment: about 300–600 lines for a tools-only server. Adding resources, subscriptions, MRTR elicitation and
Streamable HTTP, while tracking spec churn, is where the Tier-1 SDK pays off. rmcp's MSRV, license and dual-era
support all fit Axcenity. The main cost is tokio. It could run on a dedicated thread with a channel bridge to
the editor's main loop.

---

## 3. Client compatibility matrix

### 3.1 Transports and protocol features

Legend: ✅ documented as supported · ❌ documented as unsupported · n/d not documented · src = shown in the
client's source code, not its docs.

| Client | stdio | Streamable HTTP | SSE (legacy) | Tools | Resources | Prompts | Res. subscriptions / list_changed | Elicitation | Structured output | Sources |
|---|---|---|---|---|---|---|---|---|---|---|
| **Claude Code** | ✅ | ✅ (`http`) | ✅ (deprecated); also `ws` | ✅ | ✅ (`@server:uri`) | ✅ (`/mcp__srv__prompt`) | list_changed ✅ (refetch). The v2 runtime negotiates 2026-07-28 and holds a listen stream | ✅ form + URL | n/d | C-CC |
| **Claude Desktop / claude.ai** | ✅ (local config, `.mcpb`) | ✅ (remote connectors) | ✅ (legacy) | ✅ | ✅ | ✅ | **❌ subscriptions** | n/d | n/d (text and image results) | C-CB, C-CD, C-MCPB |
| **Cursor** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | n/d | ✅ (also roots, Apps) | n/d | C-CU |
| **OpenAI Codex** (CLI, IDE ext., ChatGPT desktop share one config) | ✅ | ✅ | n/d | ✅ | ✅ via `list_mcp_resources` tool (src) | n/d | n/d | ✅ (`approval_policy.granular.mcp_elicitations`) | src: passes `structuredContent` | C-CX, C-CXR, C-CXS |
| **VS Code (Copilot agent)** | ✅ | ✅ | ✅ (fallback) | ✅ | ✅ + templates | ✅ (slash) | ✅ "resource updates appear in real time"; dynamic tool discovery | ✅ (+ sampling, roots, Apps) | n/d | C-VS, C-VSG |
| **Windsurf → "Devin Desktop"** (renamed 2026-06-02) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | n/d | n/d | n/d | C-WS, C-WSF |
| **Zed** | ✅ (command) | ✅ (URL) | n/d | ✅ | n/d (src has resources types) | ✅ | tools list_changed ✅ | ❌ (not listed) | n/d | C-ZED |
| **Continue** (agent mode only) | ✅ | ✅ | ✅ | ✅ | src ✅ | src ✅ | n/d | n/d | n/d | C-CON |
| **Cline** | ✅ | ✅ (recommended) | ✅ (default if `type` omitted) | ✅ | src ✅ (`access_mcp_resource`) | n/d | n/d | n/d | n/d | C-CL |
| **LM Studio** (≥0.3.17) | ✅ (local) | ✅ (remote) | n/d | ✅ | n/d | n/d | n/d | n/d | n/d | C-LMS |
| **Jan** | ✅ (Command/Args UI) | n/d | n/d | ✅ | n/d | n/d | n/d | n/d | n/d | C-JAN |
| **Ollama itself** | ❌ No MCP client in its tool-calling docs. Use a host from the rows below | | | | | | | | | C-OLL |
| ↳ `ollmcp` (mcp-client-for-ollama, MIT) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | n/d | n/d | n/d | C-OLL |
| ↳ Open WebUI (≥0.6.31) | ❌ (use the `mcpo` proxy) | ✅ **only** | ❌ | ✅ | n/d | n/d | n/d | n/d | n/d | C-OWU |
| ↳ mcphost | ✅ | ✅ | ✅ | ✅ | n/d | n/d | n/d | n/d | n/d | **Archived; do not target** [C-MH] |
| **Gemini CLI** (reference) | ✅ | ✅ | ✅ | ✅ | ✅ (`@server://`) | ✅ (slash) | n/d | n/d | n/d | C-GEM |
| **OpenAI Responses API** (`type:"mcp"`) | ❌ remote only (public URL or Secure MCP Tunnel) | ✅ | ✅ | ✅ | n/d | n/d | n/d | n/d | n/d | C-RESP |
| **OpenAI Agents SDK** | ✅ `MCPServerStdio` | ✅ | ✅ (deprecated) | ✅ | n/d | ✅ `list_prompts`/`get_prompt` | n/d | n/d | ✅ `use_structured_content` | C-AGT |

**"Astra".** GPT-6 Astra is an **OpenAI model**, announced 2026-09-03 and available in ChatGPT, Codex and the
API [C-ASTRA]. It is **not an MCP client**, and its system card does not mention MCP [C-ASTRA]. An "Astra and
other OpenAI-based agents" user reaches Axcenity through **Codex** (stdio) or the **Agents SDK** (stdio). A
**Responses API** remote MCP connection needs an HTTP endpoint that OpenAI can reach. The web search also turned
up unrelated third-party projects named "Astra". I did not verify any of them as an MCP client.

### 3.2 Config for a stdio server named `axcenity` (`axcenity-mcp --project <path>`)

> Command name: the shapes below use `axcenity-mcp --project <path>` as a placeholder. The architecture proposal ([architecture.md §3.2](architecture.md#32-process-layout)) launches the editor binary in MCP mode, `axcenity-editor --mcp <path>`; substitute that. Only `command` and `args` change.

| Client | File / command | Shape |
|---|---|---|
| Claude Code [C-CC] | `claude mcp add axcenity -- axcenity-mcp --project <path>`. Or `.mcp.json` in the project root (shared). Local and user scope live in `~/.claude.json` | `{"mcpServers":{"axcenity":{"command":"axcenity-mcp","args":["--project","<path>"]}}}` |
| Claude Desktop [C-CD] | macOS `~/Library/Application Support/Claude/claude_desktop_config.json`, Windows `%APPDATA%\Claude\claude_desktop_config.json`, Linux `~/.config/Claude/claude_desktop_config.json` | Same `mcpServers` shape. Paths must be absolute, and backslashes are JSON-escaped on Windows. Or ship a `.mcpb` bundle [C-MCPB] |
| Cursor [C-CU] | `.cursor/mcp.json` (project) or `~/.cursor/mcp.json` | `{"mcpServers":{"axcenity":{"command":"axcenity-mcp","args":["--project","${workspaceFolder}"]}}}`. `${workspaceFolder}` is documented [C-CU] |
| Codex CLI / IDE [C-CX][C-CXR][C-CXS] | `~/.codex/config.toml`, or `.codex/config.toml` (trusted projects only). CLI: `codex mcp add axcenity -- axcenity-mcp --project <path>` | `[mcp_servers.axcenity]` / `command = "axcenity-mcp"` / `args = ["--project", "<path>"]` / optional `cwd`, `startup_timeout_sec`, `tool_timeout_sec`, `enabled_tools`, `default_tools_approval_mode = "writes"` |
| VS Code [C-VS] | `.vscode/mcp.json` or user `mcp.json` (key `servers`, `type` required). Also portable `.mcp.json` or `~/.copilot/mcp-config.json` (key `mcpServers`) | `{"servers":{"axcenity":{"type":"stdio","command":"axcenity-mcp","args":["--project","<path>"]}}}` |
| Windsurf / Devin Desktop [C-WS] | `~/.config/devin/mcp_config.json` (or `$XDG_CONFIG_HOME/devin/…`). Windows `%AppData%/devin/mcp_config.json`. Legacy discovery file: `~/.codeium/windsurf/mcp_config.json` | `{"mcpServers":{"axcenity":{"command":"axcenity-mcp","args":["--project","<path>"]}}}` |
| Zed [C-ZED] | Zed `settings.json`, key `context_servers` | `"context_servers":{"axcenity":{"command":"axcenity-mcp","args":["--project","<path>"],"env":{}}}` |
| Continue [C-CON] | `.continue/mcpServers/axcenity.yaml` (block files need `name`/`version`/`schema`). Claude/Cursor-style JSON dropped in that folder also works | `mcpServers:` / `- name: axcenity` / `type: stdio` / `command: axcenity-mcp` / `args: ["--project", "<path>"]` |
| Cline [C-CL] | `cline_mcp_settings.json` (IDE: "Configure MCP Servers"). CLI: `~/.cline/data/settings/cline_mcp_settings.json` | `{"mcpServers":{"axcenity":{"command":"axcenity-mcp","args":["--project","<path>"],"disabled":false,"autoApprove":[]}}}` |
| LM Studio [C-LMS] | In-app `mcp.json` ("Cursor notation") | Same `mcpServers` shape as Cursor |
| Jan [C-JAN] | Settings → MCP Servers → `+` | UI fields: Name `axcenity`, Command `axcenity-mcp`, Arguments `--project <path>` |
| ollmcp [C-OLL] | `ollmcp mcp add`. Writes `~/.config/ollmcp/mcp.json`, or reads a project `.mcp.json` (Claude Code-compatible) | Same `mcpServers` shape |
| Gemini CLI [C-GEM] | `~/.gemini/settings.json` or `.gemini/settings.json` | `{"mcpServers":{"axcenity":{"command":"axcenity-mcp","args":["--project","<path>"]}}}` |
| Agents SDK [C-AGT] | Code | `MCPServerStdio(params={"command":"axcenity-mcp","args":["--project", path]})` |

Takeaway: the `{"mcpServers":{name:{command,args,env}}}` JSON shape works in Claude, Cursor, Windsurf/Devin,
Cline, LM Studio, Gemini CLI, ollmcp and portable VS Code files. VS Code's own format uses `servers` and
requires `type`. Zed uses `context_servers`. Codex uses TOML.

### 3.3 Known limits, annotations and confirmation behaviour

| Client | Limits that affect server design | Annotations / confirmation |
|---|---|---|
| Claude Code [C-CC] | The full name `mcp__<server>__<tool>` must be ≤128 chars, or the tool is dropped. Top-level property names: 1–64 chars from `[A-Za-z0-9_.-]`. Schemas must validate as 2020-12. A root `anyOf`/`oneOf`/`allOf` is **flattened**, with `required` moved into the description. Output warns at 10k tokens, caps at 25k (`MAX_MCP_OUTPUT_TOKENS`), and text over 50k chars goes to a file unless `_meta["anthropic/maxResultSizeChars"]` is set (≤500k). Descriptions: 4,096 chars when loaded upfront, 16,384 via tool search; server instructions 4,096. Idle timeout for stdio is 30 min. Calls are auto-backgrounded after 2 min. On Windows, after closing stdin, the process tree is killed **300 ms** later | Per-tool permission prompts. `readOnlyHint` lets calls run in parallel. `readOnlyHint`/`idempotentHint` allow one retry after a dropped connection. `_meta["anthropic/requiresUserInteraction"]` forces a prompt on every call |
| Claude Desktop / claude.ai [C-CB][C-CR] | Tool results up to about 150,000 chars. **240 s** per tool call. Directory listing: tool names ≤64 chars, and each tool needs a `title` plus the applicable hint | `readOnlyHint: true` runs without per-call confirmation. `destructiveHint: true` always prompts [C-CR] |
| Cursor [C-CU][C-CUB] | No tool-count or name limit in current docs. Since 2026-01-06, tool descriptions sync to files and load on demand, which cut agent tokens by 46.9% in an A/B test. The older "40 tools" cap appears only in **user forum posts** (secondary) [C-CUF] | Asks approval by default. Auto-review mode runs allowlisted tools directly. Annotation use is not documented |
| Codex [C-CXR] | **`tool_input_schema_max_bytes` defaults to 5,000** (larger input schemas are compacted). Startup timeout 10 s; tool timeout 60 s. Per-tool `output_token_limit`. Stdio env is whitelisted through `env_vars` | `default_tools_approval_mode`: `auto`/`prompt`/`writes`/`approve`. **`writes` prompts only for tools not marked read-only** |
| VS Code [C-VST][C-VSG] | **At most 128 tools per chat request**. Virtual tools activate above `github.copilot.chat.virtualTools.threshold` | `title` is shown on invocation. **Read-only tools run without confirmation.** Other tools get a dialog where the user can edit inputs |
| Windsurf / Devin [C-WS][C-WSF] | No tool-count limit documented. `disabledTools` and an admin allowlist | Prompts before every MCP tool call by default |
| Zed [C-ZED] | n/d | `agent.tool_permissions.default`: `confirm` (default), `allow` or `deny`. Per-tool keys look like `mcp:axcenity:<tool>` |
| Cline [C-CL] | n/d | Per-server `autoApprove` list |
| Jan [C-JAN] | More tools use more context and slow responses | Approves every call inline unless "Allow All MCP Tool Permission" is on |
| Gemini CLI [C-GEM] | Strips `$schema` and `additionalProperties`, drops `default` inside `anyOf`, and truncates names over **63** chars. Avoid `_` in server names | Confirmation unless the server is set to `trust: true` |
| OpenAI strict schemas [C-AGT-S] | Agents SDK `convert_schemas_to_strict`: the root must not be `anyOf`, objects get `additionalProperties:false`, and all properties become `required` (best effort; it falls back to the original schema) | Responses API: `require_approval` defaults to asking; `allowed_tools` filters tools [C-RESP] |

---

## 4. Tool count, token cost and small local models

| Source | Guidance |
|---|---|
| Anthropic tool search [C-ATS] | Accuracy "degrades once you exceed 30–50 available tools." Use tool search with ≥10 tools or >10k tokens of definitions. A 5-server setup costs about 55k tokens, and search cuts that by more than 85% |
| Claude Code [C-CC] | Tool search is **on by default**: only tool names and server instructions load at start. No fixed per-server cap. `ENABLE_TOOL_SEARCH=auto` switches on at 10% of context. Put search keywords near the start of descriptions |
| OpenAI [C-FC][C-TS] | "Aim for fewer than 20 functions available at the start of a turn" (a soft suggestion). `tool_search` with `defer_loading` (gpt-5.4+). For MCP, set `defer_loading: true` on the server tool. Keep namespaces under 10 functions |
| Cursor [C-CUB] | Dynamic context discovery: names are static, details load on demand |
| VS Code [C-VST] | Hard cap of 128 tools per request, plus virtual-tool grouping |
| MCP client best practices [S17] | Hosts should switch to progressive discovery at about 1–5% of context. Deterministic tool order and stable `tools` arrays preserve prompt caching. `outputSchema` enables typed "code mode" |
| LM Studio [C-LMS][C-LMS-T] | Some servers "might use excessive amounts of tokens", which "may quickly bog down your local model and trigger frequent context overflows." Native tool support (Qwen2.5, Llama-3.1/3.2, Ministral) works better. Non-native models may emit malformed calls |
| Ollama [C-OLL-C] | Default context is **4k tokens below 24 GiB VRAM** (32k for 24–48 GiB, 256k for ≥48 GiB). Agents and coding tools should use "at least 64000 tokens" |
| Jan [C-JAN] | "Lots of open source models are not designed to use tools." Tool calling must be enabled per local model |
| BFCL (V4, updated 2026-04-12) [BFCL] | Multi-turn failure modes even for top models: missed implicit calls, ignoring the current state, needless planning [BFCL-3] |

---

## 5. Implications for Axcenity

1. **Ship stdio first** as the `axcenity-mcp` binary. Every client in the matrix supports stdio except Open WebUI and the Responses API. The spec also recommends stdio for local servers [S11]. Exit promptly on stdin EOF, and keep editor state consistent if the process is killed (Claude Code on Windows allows 300 ms) [S6][C-CC].
2. **Use `rmcp` 3.x**: Tier 1, Apache-2.0, MSRV 1.88, edition 2024, and dual-era (it answers both legacy `initialize` and 2026-07-28 `server/discover`) [R1–R7]. Isolate tokio on an MCP thread. Hand-rolling is possible, but it means maintaining two protocol eras yourself.
3. **Add Streamable HTTP second, and opt-in only**: for attaching to a running editor, and for Open WebUI or OpenAI hosted MCP (through a tunnel). Bind to 127.0.0.1, **explicitly set rmcp `allowed_origins`** (it is off by default), and require a per-session bearer token [S7][S11][R6]. Never implement HTTP+SSE.
4. **Treat tools as the only universally safe primitive.** Resources and prompts are widely supported, but Codex, Cline and Zed reach them through generated tools or don't document them. Expose anything important as a tool, and mirror it as a resource or prompt where that adds value [§3.1].
5. **Treat these as optional**: resource subscriptions (**Claude doesn't support them** [C-CB]), elicitation (Claude Code, Cursor, VS Code and Codex only), and MCP Apps. **Don't use** sampling, roots or logging, which are deprecated [S10]. Take the project path from `--project` rather than Roots.
6. **Write portable tool schemas**: a plain `type:"object"` root with no top-level `anyOf`/`oneOf`/`allOf`, property names matching `[A-Za-z0-9_.-]{1,64}`, input schemas **under about 5 KB** (the Codex default), and no reliance on `additionalProperties` or `$schema` (Gemini strips them) [C-CC][C-CXR][C-GEM][C-AGT-S].
7. **Tool names**: snake_case, ≤64 chars to meet Anthropic directory review, and short enough that `mcp__axcenity__<tool>` stays ≤128 and Gemini's 63-char limit holds [C-CR][C-CC][C-GEM].
8. **Keep the default surface small**, around 15–20 tools. Group with a domain verb plus an `action`/`kind` argument rather than many near-duplicate tools. Allow profiles such as `--tools read-only` or `--tools scene`. VS Code caps requests at 128 tools, OpenAI suggests fewer than 20, Anthropic sees degradation above 30–50, and local models run at 4k context by default [C-VST][C-FC][C-ATS][C-OLL-C].
9. **Set annotations on every tool**: `title`, plus `readOnlyHint`, `destructiveHint` or `idempotentHint`. They directly control prompting in Claude, VS Code and Codex `writes` mode, and parallelism and retries in Claude Code [C-CR][C-VSG][C-CXR][C-CC]. Return `structuredContent` with an `outputSchema`, **and** the same JSON as text [S8].
10. **Keep results small and paged.** Summaries should stay under about 10k tokens (Claude Code's warning threshold). Return a `resource_link` or handle for bulk data. Use `isError: true` with actionable text for validation failures. Long operations (bake, build) need progress notifications or a handle the model polls, because Codex times out tool calls at 60 s and Claude Desktop at 240 s [C-CC][C-CXR][C-CB][S8].

---

## Sources

**Spec**
- S1 https://modelcontextprotocol.io/specification/versioning
- S2 https://modelcontextprotocol.io/specification/2026-07-28/changelog
- S3 https://modelcontextprotocol.io/specification/2025-11-25/changelog
- S4 https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning
- S5 https://modelcontextprotocol.io/specification/2026-07-28/basic/transports
- S6 https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio
- S7 https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http
- S8 https://modelcontextprotocol.io/specification/2026-07-28/server/tools
- S9 https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/schema/2026-07-28/schema.ts
- S10 https://modelcontextprotocol.io/specification/2026-07-28/deprecated
- S11 https://modelcontextprotocol.io/specification/2026-07-28/basic/security_best_practices
- S12 https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/subscriptions
- S13 https://modelcontextprotocol.io/specification/2026-07-28/server/discover
- S14 https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle
- S15 https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/ping
- S16 https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/schema/2025-11-25/schema.ts
- S17 https://modelcontextprotocol.io/docs/2026-07-28/develop/clients/client-best-practices

**Rust**
- R1 https://crates.io/crates/rmcp
- R2 https://github.com/modelcontextprotocol/rust-sdk
- R3 https://docs.rs/crate/rmcp/3.9.0/source/Cargo.toml.orig
- R4 https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/CHANGELOG.md
- R5 https://docs.rs/crate/rmcp/3.9.0/source/src/model.rs (`ProtocolVersion`) and `src/service/server.rs` (`negotiate_protocol_version`)
- R6 https://docs.rs/crate/rmcp/3.9.0/source/src/transport/streamable_http_server/tower.rs
- R7 https://modelcontextprotocol.io/docs/sdk
- R8 https://crates.io/crates/rust-mcp-sdk · https://crates.io/crates/pmcp · https://crates.io/crates/tower-mcp
- R9 https://docs.rs/crate/rmcp/3.9.0/source/src/transport/async_rw.rs

**Clients**
- C-CC https://code.claude.com/docs/en/mcp
- C-CB https://claude.com/docs/connectors/building
- C-CR https://claude.com/docs/connectors/building/review-criteria
- C-CD https://modelcontextprotocol.io/docs/develop/connect-local-servers
- C-MCPB https://claude.com/docs/connectors/building/mcpb
- C-CU https://cursor.com/docs/context/mcp
- C-CUB https://cursor.com/blog/dynamic-context-discovery
- C-CUF https://forum.cursor.com/t/tools-limited-to-40-total/67976 (user forum; secondary, historical)
- C-CX https://learn.chatgpt.com/docs/extend/mcp?surface=cli (redirect target of developers.openai.com/codex/mcp)
- C-CXR https://learn.chatgpt.com/docs/config-file/config-reference
- C-CXS https://github.com/openai/codex: `codex-rs/cli/src/mcp_cmd.rs`, `codex-rs/core/src/tools/handlers/mcp_resource/list_mcp_resources.rs`, `codex-rs/core/src/mcp_tool_call.rs`, `codex-rs/tools/src/mcp_tool.rs`, `codex-rs/codex-mcp/Cargo.toml`
- C-VS https://code.visualstudio.com/docs/agents/reference/mcp-configuration
- C-VSG https://code.visualstudio.com/api/extension-guides/ai/mcp
- C-VST https://code.visualstudio.com/docs/agents/run/tools
- C-WS https://docs.devin.ai/windsurf/plugins/cascade/mcp
- C-WSF https://docs.devin.ai/desktop/devin-desktop-faq · https://docs.devin.ai/desktop/devin-local/customizations
- C-ZED https://zed.dev/docs/ai/mcp
- C-CON https://docs.continue.dev/customize/deep-dives/mcp, and source at https://github.com/continuedev/continue/tree/main/core/context/mcp
- C-CL https://docs.cline.bot/mcp/configuring-mcp-servers, and source at https://github.com/cline/cline/blob/main/apps/vscode/src/shared/tools.ts
- C-LMS https://lmstudio.ai/docs/app/mcp · C-LMS-T https://lmstudio.ai/docs/developer/openai-compat/tools
- C-JAN https://docs.jan.ai/desktop/mcp
- C-OLL https://docs.ollama.com/capabilities/tool-calling · https://github.com/jonigl/mcp-client-for-ollama
- C-OLL-C https://docs.ollama.com/context-length
- C-OWU https://docs.openwebui.com/features/mcp
- C-MH https://github.com/mark3labs/mcphost
- C-GEM https://geminicli.com/docs/tools/mcp-server/
- C-RESP https://developers.openai.com/api/docs/guides/tools-connectors-mcp
- C-AGT https://openai.github.io/openai-agents-python/mcp/ · C-AGT-S https://github.com/openai/openai-agents-python/blob/main/src/agents/strict_schema.py
- C-FC https://developers.openai.com/api/docs/guides/function-calling
- C-TS https://developers.openai.com/api/docs/guides/tools-tool-search
- C-ATS https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool
- C-ASTRA https://deploymentsafety.openai.com/gpt-6-astra · https://community.openai.com/t/introducing-gpt-6-astra-the-most-intelligent-and-aligned-model-in-the-world/1394703
- BFCL https://gorilla.cs.berkeley.edu/leaderboard.html · BFCL-3 https://gorilla.cs.berkeley.edu/blogs/13_bfcl_v3_multi_turn.html

**Not verified this session:** the OpenAI Structured Outputs "Supported schemas" section, which the docs site did
not serve. The strict-schema rules above come from the Agents SDK source instead. Linux support for Claude Desktop
is also unresolved: the MCP docs list a Linux config path [C-CD], while the MCPB page lists only macOS and
Windows [C-MCPB].
