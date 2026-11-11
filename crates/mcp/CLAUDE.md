# `mcp`

- The editor's MCP server, engine-free: `spec.rs` (`ToolSpec`, `Class`, `Profile`, `Param`), `args.rs` (checking a call's arguments), `error.rs` (`ToolError`, one text shape), `server.rs` (`rmcp` on its own thread, on stdio or a TCP connection for the open editor; calls go to the editor as `Job`s over a channel), `extras.rs` (prompts, and resources that run a read tool), `text.rs` (compact numbers, token estimate), `safety.rs` (`Mode`, confirm tokens bound to one call). Behavior: `docs/editor/mcp.md`; design: `docs/research/mcp/architecture.md`.
- The tools themselves are the editor's (`editor/src/app/mcp/`, a child of `app` so they reach `EditorApp`). Editor-only: the player and server must never depend on this crate (it brings `tokio`).
- Keep schemas flat (no `oneOf`/`$ref`) and summaries one sentence; errors say what to do next.
