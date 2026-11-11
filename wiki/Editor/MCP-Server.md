---
title: MCP server for AI clients
type: system
status: current
implementation: implemented
crates: [mcp, editor]
source: [crates/mcp/src/lib.rs, crates/mcp/src/server.rs, crates/mcp/src/spec.rs, crates/mcp/src/safety.rs, crates/editor/src/app/mcp.rs]
docs: [docs/editor/mcp.md, docs/research/mcp/README.md, docs/research/mcp/architecture.md, crates/mcp/CLAUDE.md]
verified: 2026-11-11
verified_commit: 19c7179d
tags: [system/editor, ai, mcp]
---

# MCP server for AI clients

> [!summary]
> `axcenity-editor --mcp <project>` opens a project **without a window** and serves it to AI clients (Claude Code, Cursor, Codex, local models) over the **Model Context Protocol** on stdin/stdout (PRs #102, #103, #106, #108, #109; 2026-11-11). The protocol half is the engine-free **`mcp`** crate (`rmcp` on its own thread, tool specs with flat JSON schemas, argument checks, one `ToolError` shape, safety modes and confirm tokens); the **tools are the editor's** (`editor/src/app/mcp.rs` and `app/mcp/`), handed to the thread that owns `EditorApp` as `Job`s, so they use the same code paths as the panels. **Editor only**: the player and server never link it (it brings `tokio`). **Implemented.**

This is *distinct from this wiki's agent instructions*: the repository's `AGENTS.md` and this wiki tell agents working on the **engine's source** how to behave ([Wiki guide](../Getting-Started/Wiki-Guide.md)); the MCP server lets agents work on a **game project** through the editor.

## Shape

```mermaid
sequenceDiagram
  participant C as AI client
  participant M as mcp crate (rmcp thread)
  participant E as EditorApp thread
  C->>M: tools/call (stdio)
  M->>M: check args against ToolSpec, mode, confirm token
  M->>E: Job
  E->>E: tool in app/mcp/ (same code as panels; undoable edits)
  E-->>M: result or ToolError
  M-->>C: compact text result
```

- **Profiles** (`lite`, `standard`, `full`) choose which tools are listed; **modes** (`read`, `edit`, `full`) choose what they may do; destructive calls need a confirm token bound to that one call.
- Tools include reading the project, undoable object edits, `script_read`/`script_write`, `project_check`, `batch`, `object_duplicate`, `material_edit`, `env_set`, `game_run`, with a log and a checkpoint. Object edits stay in memory until `project_save`.
- Both MCP protocol eras are served (the `initialize` handshake and the stateless 2026-07-28 one).

## Development guidance

- Keep schemas flat (no `oneOf`/`$ref`), summaries one sentence, errors that say what to do next.
- New tools go in `editor/src/app/mcp/` (one file each) and go through `EditorApp` paths so undo and history stay consistent.
- `scripts/check-platform.sh` checks the player doesn't link it.

## Related documentation

`docs/editor/mcp.md` (running it, connecting clients, the tool list), `docs/research/mcp/` (design, client compatibility, capability map, tool inventory); [Editor architecture](Editor-Architecture.md)
