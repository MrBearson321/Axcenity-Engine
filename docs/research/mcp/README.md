# MCP toolkit research (Phases 1–2)

Paul's brief "Axcenity Engine – MCP Toolkit & AI Integration Architecture" (2026-11-11) asks for an MCP subsystem that lets AI clients (Claude Code, Cursor, Codex, local models, …) operate the editor through a small, token-efficient, safe set of tools. This folder is the audit and the proposal. **Nothing is implemented until Paul approves the decisions below.**

| File | What it answers (brief's "Expected Initial Deliverable") |
|---|---|
| [engine-capability-map.md](engine-capability-map.md) | 1. Map of existing engine functionality, what to expose, missing capabilities, what can't be exposed safely |
| [architecture.md](architecture.md) | 2. MCP architecture · 4. token strategy · 5. high vs low level · 7. safety and permissions · 8. roadmap and tests · simplifications |
| [tool-inventory.md](tool-inventory.md) | 3. Categorized tools with names, classes, parameters, results; resources and prompts |
| [client-compatibility.md](client-compatibility.md) | 6. Client compatibility: spec revisions, transports, features, config formats, limits |

## Summary

- The editor already has what an AI layer needs: generational Instance IDs that survive delete and undo, validated property sets, class reflection with docs for every property, a change log, an undo history with recorders for every kind of edit, a copy-run-diff transaction pattern (editor plugins), project checks and fixes, version checkpoints, a game-test runner, and a headless `EditorApp` that every editor test already uses. The toolkit is a thin, well-tested layer over those, not new engine machinery.
- **Recommended architecture**: clients launch `axcenity-editor --mcp <project>` over stdio. It relays to the editor if the user has that project open with AI allowed (live: they watch and can undo), otherwise it opens the project headless with the same code. One set of tools, one validation path, one undo history.
- **About 20 tools** in three profiles (`lite` 11 for small local models, `standard` 21, `full` 28), one shared parameter vocabulary, compact text results with non-default properties only, change cursors instead of re-reading the scene, and a token budget enforced by a test.
- **Safety**: every write is one labelled undo step (`AI (Claude Code): …`); deletes and other high-impact calls return a preview and need a `confirm` token; a version checkpoint before each AI session; an audit log; no Lua execution, no files outside the project, no network.

## Decisions for Paul

Each has a recommended default. Approving "defaults" approves all of them.

| # | Decision | Recommended | Alternatives |
|---|---|---|---|
| D1 | Hosting | **Hybrid**: `axcenity-editor --mcp` relays to a live editor, else runs headless (architecture §3) | Live editor only (needs the GUI open; no cloud agents) · headless only (no watching/undo in the editor) |
| D2 | MCP library | **Official `rmcp` 3.x SDK**, stdio only, `tokio` on the MCP thread of the editor binary (never the player or server). It speaks both live spec eras (2026-07-28 stateless and 2025-11-25) (§3.4) | Hand-written JSON-RPC on `serde_json`: no `tokio`, but we maintain two protocol eras ourselves |
| D3 | Default permissions | **`edit` mode**: reads and reversible writes run; deletes of > 5 objects or any script, landscape replacement, saves and project-wide changes need preview + confirm. Live-editor AI connections **off until turned on per project** (§7) | `full` by default · confirm every write |
| D4 | Result format | **Compact text lines**; structured JSON output only for `project_info` and `history changes` (§5) | Structured JSON for every result (≈ 2× tokens) |
| D5 | Session safety net | **Automatic version checkpoint** before each AI session's first write, plus `.axcenity/mcp-log.jsonl` audit log (§7.4) | Log only · neither |
| D6 | Lua execution | **No `run_lua` tool**: agents write script files and run Play/tests (§2, capability map §4) | Opt-in `run_lua` behind `full` mode |
| D7 | Tool set | **The inventory as proposed**, `standard` profile by default (tool-inventory.md) | Fewer tools (fold `env_set`/`terrain_edit` into `object_set`) · more granular tools |
| D8 | Order | **Headless + read-only first** (step 3.2), writes (3.3), then the live editor host (3.4) after M12 lands (architecture §12) | Live editor first |
| D9 | AI saving | **`project_save` allowed with confirm**, in live and headless mode | Live mode: only the user saves |

## Process

- Audit on `main` at `b3297ca`, 2026-11-11, in the cloud. Client facts in `client-compatibility.md` come from primary sources (spec, SDK repos, vendor docs) as of the same date.
- Related threads: gap analysis/roadmap (G1–G16, PRs #89/#92; G2 scripts in objects #91; G4 luau-lsp after M12), Toolbox (`crates/toolbox`, PR #90). MCP wraps their results; it doesn't duplicate them.
