# MCP server measurements

These are step 7's numbers for the MCP server (`axcenity-editor --mcp`; see [editor/mcp.md](../editor/mcp.md)). They replace the estimates in the [architecture](../research/mcp/architecture.md#9-high-level-versus-low-level-tools).

How the numbers were taken:

- **Server side:** `app/mcp/benchmarks.rs`, run as
  ```
  cargo test -p axcenity-editor --release --bin axcenity-editor -- mcp::benchmarks --ignored --nocapture
  ```
  on the 4-vCPU cloud container at main `811dda12` plus PR #185 and this change. The container is about 2.5× slower than Paul's desktop, so divide the times by about 2.5 for his PC.
- **Small-model runs:** LM Studio on Paul's desktop (Ryzen 9 9950X3D, RX 9070 XT, 16 GB). Each model did the 7 lite tasks 3 times, with 32k context, temperature 0.2, and at most 12 calls and 3 minutes per task.

Token counts are the server's estimate (`text::estimate_tokens`: one token per 3.5 characters, rounded up). [Calibration](#calibration) compares the estimate with real tokenizers.

## Definitions

What a client pays every turn: the `tools/list` JSON for a profile, plus the instructions.

| Profile | Tools | Tokens | Budget (test) |
|---|---|---|---|
| `lite` | 13 | 2,095 | 2,100 |
| `standard` | 22 | 4,362 | 4,500 |
| `full` | 29 | 5,545 | 6,500 |
| instructions | | 134 | 150 |

## Tools

Each tool is called once on a copy of `examples/hello-part`, the way a model calls it first. Times are in-process, in milliseconds, and leave out the transport.

| Tool | Call | Result tokens | Time (ms) |
|---|---|---|---|
| project_info | `{}` | 60 | 0.22 |
| scene_inspect | `{}` | 46 | 0.04 |
| scene_inspect | `{"depth":3,"root":"game"}` | 152 | 0.04 |
| scene_inspect | `{"class":"BasePart"}` | 16 | 0.04 |
| object_get | `{"id":"Workspace/Block"}` | 32 | 0.07 |
| object_get | `{"detail":"full","id":"Workspace/Block"}` | 161 | 0.04 |
| docs | `{}` | 821 | 0.05 |
| docs | `{"class":"Part"}` | 716 | 2.56 |
| script_read | `{}` | 17 | 0.05 |
| project_check | `{"scope":"scripts"}` | 8 | 0.77 |
| project_check | `{"scope":"assets"}` | 8 | 1,137 |
| history | `{"action":"changes"}` | 10 | 0.04 |
| asset_find | `{}` | 626 | 0.31 |
| project_settings | `{"setting":"input"}` | 144 | 0.04 |
| object_create | a red Block named Crate | 20 | 0.94 |
| object_set | two props | 23 | 0.07 |
| object_duplicate | 3 copies in a row | 27 | 0.07 |
| object_delete | one part | 14 | 0.05 |
| script_write | a new 2-line script | 28 | 0.97 |
| material_edit | create and assign | 40 | 0.29 |
| env_set | `{"preset":"sunset"}` | 47 | 0.10 |
| terrain_edit | generate (first call: asks first) | 68 | 0.08 |
| game_run | `{"seconds":1}` | 33 | 19 |
| history | `{"action":"undo"}` | 11 | 0.10 |

What this shows:

- **Results are short.** A typical answer is 10–60 tokens. The large ones are listings a model asks for once: `docs` with no class (all class names and kinds), a class's properties, and `asset_find` with no filter.
- **Calls are quick.** Almost every call takes well under a millisecond.
- **Two calls are slow.** `game_run` takes about 19 ms on top of the seconds it plays. `project_check` on assets takes about 1.1 s because it loads every model file to check it: `hello-part` holds 59 MB of models. This is the Asset Check panel's own scan, shared with the editor, and its cost grows with the model files, not with the scene. S10k's assets check in about 0.1 s.

## Reference workflows

Each route runs on a fresh copy of the project. Tokens count what the model sends (the arguments) plus what comes back, over all calls.

| Workflow | Route | Calls | Tokens | Time (ms) |
|---|---|---|---|---|
| Small hut (model, floor, 4 walls, roof, light) | 8 × `object_create` | 8 | 347 | 1.3 |
| | 1 `batch` | 1 | 319 | 1.1 |
| Sunset look | `docs` + `object_set` + 4 × `object_create` | 6 | 261 | 1.0 |
| | 1 `env_set` | 1 | 53 | 0.9 |
| Door script and try it | `script_write`, `game_run`, `script_read` | 3 | 156 | 20 |

These confirm the architecture's conclusion:

- **`batch` saves round trips, not tokens.** It takes 7 fewer turns, but the arguments are the same. Each turn also re-sends the conversation, which is where the real saving is.
- **`env_set` saves the engine knowledge.** It costs 5 times fewer tokens and 5 fewer calls, and the model doesn't need to know which effect classes and properties make a sunset.
- **A script workflow is already minimal.**

## In the open editor

A client of the open editor (Settings ▸ AI connections) talks to it over a loopback connection. Its calls run at the start of the next frame.

| Measure | Value |
|---|---|
| Round trip for `project_info`, editor drawing at 60 fps (median of 30) | 16.9 ms |
| Round trip for `project_info`, frames back to back as the waker asks (median of 30) | 0.31 ms |
| Idle frame with a client connected (the drain finds nothing) | 0.1 µs |
| Idle frame with no client | returns at once (one lock and an empty check) |

The first measurement found a bug. Both sides of the connection had Nagle's algorithm on, so each short answer waited for the other side's delayed ACK. A round trip took 44–51 ms whatever the frame rate. Both ends now set `TCP_NODELAY`, which brings it to 0.31 ms. In the editor, the waker asks for a repaint when a call arrives, so an idle editor answers within one frame.

A frame runs calls for up to 4 ms (`live::BUDGET`). A call that takes longer, such as `game_run`, holds that frame.

The change log keeps the last 20,000 changes (`changes::KEPT`). Each is a small fixed-size record of about 48 bytes: its number, who made it, and the change itself (IDs and a static property name). That's about 1 MB at most, and the log starts over when the first client of a new session arrives.

## Large scene (S10k)

[S10k](../benchmarks.md) is the standard measurement scene: about 10,000 parts plus spinners, characters and scripts. Times are in-process.

| Call | Result tokens | Time (ms) |
|---|---|---|
| Open the project (what `--mcp` does before it can answer) | | 4,025 |
| `project_info` | 61 | 0.4 |
| `scene_inspect` (the Workspace tree, 2 levels, folded) | 300 | 3.2 |
| `scene_inspect` `class: BasePart` (first page of 60) | 599 | 3.6 |
| the same, `cursor: 5000` | 708 | 3.8 |
| `scene_inspect` `name: Spinner` | 19 | 0.6 |
| `scene_inspect` `near: [0,0,0]`, `radius: 20` | 705 | 3.2 |
| `project_check` | 7 | 95 |
| `history` `changes` | 10 | 0.03 |
| `object_create` (the first write also makes the session's safety copy) | 22 | 27 |
| `object_delete` | 15 | 20 |
| `history` `undo` | 10 | 2.0 |

What this shows:

- **Folding and paging work.** No result passes about 700 tokens, however many parts match.
- **A write costs 20–30 ms.** The undo step's before-image of a 10,000-part world is most of it. That is fine for a model's pace.
- **Opening the project is the real cost.** It takes about 4 s here, or about 1.6 s on Paul's PC. A client that starts `--mcp` waits that long for its first answer. A client of the open editor doesn't wait at all.

## Small local models

Lite profile, 7 tasks × 3 runs per model, on Paul's PC. "Before" is main before #178. "After" is main at `811dda12`, which includes #178 and #181.

| Task | gpt-oss-20b | Qwen3 8B | Llama 3.1 8B |
|---|---|---|---|
| 1. What is in the Workspace? | 3 → 3 | 3 → 3 | 3 → 3 |
| 2. Add a red crate | 3 → 3 | 3 → 3 | 0 → 0 |
| 3. Make Block blue and raise it | 3 → 3 | 3 → 3 | 0 → 0 |
| 4. Delete Block | 2 → 3 | 2 → 3 | 0 → 0 |
| 5. A script that prints on touch | 0 → 1 | 1 → 3 | 0 → 0 |
| 6. Run the game, any errors? | 3 → 3 | 3 → 3 | 3 → 3 |
| 7. Undo, then save | 3 → 3 | 3 → 3 | 0 → 0 |
| **Total (of 21)** | **17 → 19** | **18 → 21** | **6 → 6** |
| Tokens per task | 8,300 → 7,500 | 12,000 → 9,700 | 11,400 → 10,600 |

A rerun with #184 (lists and numbers sent as text are accepted, the `script.Parent` warning, a script file given as an object) fixed what it targeted. Llama's quoted colors went through, and gpt-oss read both new script messages. The totals barely moved, though:

- **gpt-oss:** 1/3 on task 5. It read the warnings but didn't change its code. PR #185 puts the exact `script_write` call in the warning.
- **Llama 3.1 8B:** about 6/21. Its failures are the model's own:
  - it guesses IDs and deletes the wrong object after reading the preview;
  - it moves "up" along Z;
  - it leaves out names;
  - it sends `props` as Python-dict text;
  - LM Studio rejected 10 of its 21 runs because the model's tool calls didn't parse.

Our messages can't fix any of these.

## Confusing-tool review

This review draws on all four runs (gpt-oss-20b, Qwen3 8B and Llama 3.1 8B, before and after the fixes) and on the GUI check with Claude Haiku. Each row is a message or schema that misled a model more than once, and what changed.

| What misled | Seen | Change | PR |
|---|---|---|---|
| `object_create kind: "Script"` said "no kind Script" | gpt-oss, every run of task 5 | Scripts aren't kinds; the fix line names `script_write` and how code reaches a part | #178 |
| `"tag": ""` (an unwanted filter sent empty) searched for the empty tag: "0 matches" | gpt-oss, 7 times | An empty filter counts as not given | #178 |
| `class: "Block"` said "no class Block" | Qwen | Says it's a kind whose class is Part | #178 |
| `project_save` with nothing to save still asked for a confirm token | gpt-oss | Says "nothing to save" and asks nothing | #178 |
| `object_set` with no real change said "changed … nothing changed" | gpt-oss | Says the object already had those values | #178 |
| A syntax error came in the middle of a successful `script_write` result, and resending the same code said "already like that" | Qwen, 7 resends | The error comes first; resending is an error that names the line | #181 |
| A bare name (`"Block"`) said "game has nothing called Block" | Qwen, Llama | Suggests `"Workspace/Block" (ID 12)`; this fixed the next call 7 of 7 times | #181 |
| A name searched as a tag returned "0 matches" | Qwen | Says no object has that tag and how many have that name | #181 |
| A new script given a path under a part | Qwen | Says a new script takes `name` and `side` | #181 |
| Lists and numbers sent as JSON text (`"[1,0,0]"`) refused | Llama, 30 times in one task | Parsed when the text fits the parameter | #184 |
| `script.Parent.Touched` passed "syntax ok", but the script lives in its service | gpt-oss, 2 of 3 runs | A warning, and #185 adds the exact call that fixes it | #184, #185 |
| A script's file used as an object id suggested "ServerScriptService" | gpt-oss, 4 times | Names the script's object and ID | #184 |
| Making a script whose name exists said "give it as path" | gpt-oss | Gives the exact path to call with | #185 |

Schemas: none needed simplifying. Every schema is flat and under 2 KB (`every_tool_is_short_flat_and_in_budget`). No run sent an unknown parameter more than once. What tripped models was the values and the messages, not the shapes.

What's left is up to the model and isn't something the server can fix: guessed IDs, the up axis, and malformed tool calls. Results already show IDs, and the instructions say Y is up. Destructive calls still ask first and stay undoable, so a model that guesses wrong loses nothing for good.

## Calibration

The estimate counts one token per 3.5 characters. To check it, we used the small-model runs: their first request to each model held only the instructions (466 characters), the 13 `lite` tools in OpenAI form (7,089 characters) and a short task (13 to 70 characters). The server estimates that content at about 2,250 tokens. LM Studio's `usage.prompt_tokens` gives each model's real count, including its chat template:

| Model | Prompt tokens (first request) | Characters per token | Compared with the estimate |
|---|---|---|---|
| gpt-oss-20b | about 1,170 (bounds 1,059 to 1,221) | about 6.5 | half: its Harmony template rewrites the JSON schemas into a compact TypeScript-like form |
| Qwen3 8B | about 2,180 (bounds 2,066 to 2,229) | about 3.5 | matches |
| Llama 3.1 8B | 2,840 (exact) | about 2.7 | about 25% more: its template adds a tool preamble and spaced JSON |

Gpt-oss and Qwen are medians of tasks with two requests, bounded from the per-task usage total. Llama's number comes from runs whose second turn LM Studio rejected, so it is a single request and exact. Its two-request estimate (2,832) agrees with that within 0.3%.

The 3.5 ratio holds for Qwen-style tokenizers, overstates for gpt-oss and understates Llama by about a quarter. So the budgets are a fair guide for comparing profiles. `lite` costs at most about 3,000 tokens in any of the three models, which leaves most of an 8k context for the work. Claude's tokenizer wasn't measured: the container can't download tokenizers, and the runs used only local models.
