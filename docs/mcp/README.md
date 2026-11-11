# Using AI assistants with Axcenity (MCP)

Axcenity's editor can serve a project to AI assistants such as Claude Code, Cursor, Codex, VS Code, or a local model in LM Studio. It uses the Model Context Protocol (MCP), the standard way these assistants call outside tools. The assistant can look at your scene, build and change objects, write scripts, check for problems and play the game for a few seconds. Every change it makes goes through the editor's own code and is one undo step.

This page is for people using it. How it works inside, and every tool's parameters, is in [editor/mcp.md](../editor/mcp.md).

## Quick start

1. **Check it works for your project.** In a terminal, run the editor with `--mcp --check` and your project folder:

   ```
   axcenity-editor --mcp --check "/path/to/My Game"
   ```

   Where the editor is:
   - **Windows:** `"%LOCALAPPDATA%\Programs\Axcenity Engine\axcenity-editor.exe"`.
   - **macOS:** `"/Applications/Axcenity Editor.app/Contents/MacOS/axcenity-editor"`.
   - **Linux (.deb, .rpm, Arch):** `axcenity-editor` is on your `PATH`. With the tarball or AppImage, use the file you unpacked.

   The check opens the project without a window and prints `ok`, `warn` or `FAIL` lines. It then prints the exact lines to paste into each assistant, with full paths filled in. It exits with 1 if anything failed.

2. **Add it to your assistant.** Paste the line the check printed for your client (see [Clients](#clients)). The assistant starts the editor itself when it needs it. You don't run the server by hand.

3. **Ask for something.** For example: "Build a small wooden hut with a door near the spawn point." Or: "Why does my door script error when I touch it?"

4. **Keep it.** Object edits stay in memory until the assistant calls `project_save`, which asks first. Scripts are saved as soon as they're written. Open the project in the editor afterwards to see the result.

> **Don't edit the same project in the editor at the same time.** The server is a separate copy of the editor without a window, so the open editor doesn't see its changes until you reopen the project. If both save, the last save wins. Connecting to the editor window you have open is planned (step 3.4 of the [design](../research/mcp/architecture.md)).

## Clients

`--check` prints these lines with your paths filled in. Here `EDITOR` is the editor's full path and `PROJECT` is your project folder's full path. In JSON, a Windows backslash is written `\\`.

| Client | Where it goes | What to add |
|---|---|---|
| Claude Code | a terminal | `claude mcp add axcenity -- EDITOR --mcp PROJECT`, or `.mcp.json` in the project with the JSON below |
| Claude Desktop | `claude_desktop_config.json`. Windows: `%APPDATA%\Claude\`. macOS: `~/Library/Application Support/Claude/`. Linux: `~/.config/Claude/` | `{"mcpServers": {"axcenity": {"command": "EDITOR", "args": ["--mcp", "PROJECT"]}}}` |
| Cursor | `.cursor/mcp.json` in the project, or `~/.cursor/mcp.json` | the same `mcpServers` JSON |
| VS Code (Copilot agent mode) | `.vscode/mcp.json` | `{"servers": {"axcenity": {"type": "stdio", "command": "EDITOR", "args": ["--mcp", "PROJECT"]}}}` |
| Codex (CLI and IDE) | `~/.codex/config.toml`, or `codex mcp add axcenity -- EDITOR --mcp PROJECT` | `[mcp_servers.axcenity]`, then `command = "EDITOR"` and `args = ["--mcp", "PROJECT"]` |
| Windsurf / Devin Desktop, Cline, Gemini CLI | that client's MCP settings file | the same `mcpServers` JSON |
| Zed | `settings.json` | `"context_servers": {"axcenity": {"command": "EDITOR", "args": ["--mcp", "PROJECT"]}}` |
| LM Studio (0.3.17 or later) | LM Studio's `mcp.json` (edit it from the app) | the same `mcpServers` JSON, plus `"--profile", "lite"` in `args` |
| Jan | Settings → MCP Servers → `+` | Command `EDITOR`, Arguments `--mcp PROJECT --profile lite` |
| Ollama | Ollama has no MCP client of its own. Use `ollmcp` (`ollmcp mcp add`) or another host from this table | the same `mcpServers` JSON |

Open WebUI and OpenAI's Responses API only reach servers over HTTP. This server speaks stdio only, so they can't use it yet.

## Choosing a profile

A profile decides which tools the assistant sees. Each tool's description costs context tokens every turn, so fewer tools leave more room for the work. Add `--profile <name>` to the arguments.

| Profile | Tools | Description cost | For |
|---|---|---|---|
| `lite` | 13: look, make, change, delete, scripts, checks, play, undo, save | about 2,100 tokens | local models with 8k–32k context |
| `standard` (default) | 21: adds batches, duplicating, materials, environment, assets, terrain, animation | about 3,300 tokens | Claude Code, Cursor, Codex and other cloud models |
| `full` | 27: adds levels, prefabs, solid modeling, stylesheets, input/collision/strings and asset rename/delete | about 4,400 tokens | assistants that should be able to do everything |

### Local models

- **Use a model with native tool calling.** Examples are Qwen 2.5 or later, Llama 3.1 or later, and Ministral. Models without it often send broken calls. In Jan and LM Studio, turn tool use on for the model.
- **Give it enough context.** Ollama defaults to 4k tokens of context on GPUs under 24 GB, which is too small even for `lite`. Set `num_ctx` to 32k or more.
- **Start with the prompts.** Many clients show the server's prompts as commands, so a small model gets a fixed recipe instead of having to plan:
  - **`axcenity_build`** asks what to build, then looks, plans sizes, builds, checks and saves.
  - **`axcenity_fix`** runs the checks and a short Play, fixes what they find, and checks again.

  In Claude Code they are `/mcp__axcenity__axcenity_build` and `/mcp__axcenity__axcenity_fix`.

## What the assistant can and can't do

- **Undo.** Every change is one undo step labelled `AI (<assistant>): …`. The assistant can undo or redo only its own steps (`history`). Ask it "what changed?" and it can list what it changed and what someone else changed since it last looked.
- **Asks first.** Deleting many objects, saving, replacing terrain and deleting assets show a preview first. The assistant has to call again with a confirm token. Most clients also ask you before any tool that changes things, and let read-only tools run without asking.
- **Modes.** `--mode read` lets it only look. `--mode edit` (the default) runs edits and asks first for destructive ones. `--mode full` runs everything without asking.
- **A safety copy.** Before the assistant's first change, the project files are kept as a pinned version, "Before AI session (…)", in File ▸ Version History…. You can roll the whole session back.
- **A log.** Every call is a line in `.axcenity/mcp-log.jsonl` in the project. It is never exported.
- **Stays in the project.** No Lua runs outside the game's sandbox, and there is no shell or network access. The assistant only writes files inside the project. It reads outside the project only from folders you name with `--allow-import <folder>`, for `asset_import`. It can't change services, Players or locked objects.
- **Resources.** Clients that attach resources can use `axcenity://project`, `axcenity://scene`, `axcenity://object/<id or path>`, `axcenity://docs/<class>` and `axcenity://script/<path>`. In Claude Code, type `@axcenity:` to pick one. Each reads the same as the matching tool.

## Troubleshooting

| What you see | What to do |
|---|---|
| The assistant says the server failed to start | Run the `--check` line in a terminal. A `FAIL` line says why. |
| `could not open a project in …` | Point it at the folder that holds `world.json`, not a file inside it. |
| The assistant's changes are gone | It didn't save. Ask it to call `project_save` before it finishes. |
| The editor doesn't show the assistant's changes | Close and reopen the project in the editor after the assistant saves (see the note in Quick start). |
| `error not_allowed: …` | The mode doesn't allow it. The message names the `--mode` that would. |
| `error needs_confirm: preview: …` | The assistant should read the preview and call again with the token. Some small models stop here, so tell it to go ahead. |
| Windows: the JSON config is rejected | Write each `\` in a path as `\\`, or copy the line `--check` printed. |
| Codex times out starting it | Opening a big project can take longer than Codex's 10-second start limit. Add `startup_timeout_sec = 30` under `[mcp_servers.axcenity]`. |
