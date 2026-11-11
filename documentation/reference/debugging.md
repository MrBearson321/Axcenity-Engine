---
description: Every debugging and profiling tool in one place, the game test API with its matchers and Test helpers, command-line runs, benchmarks, logs, and the AI assistant connection.
icon: gauge
samples: [hello-character, hello-script]
---

# Debugging and profiling

A walkthrough is in [Debugging and performance](../learn/debugging-and-performance.md). This page lists the tools and the game test API.

## Tools

| Tool | Where | What it shows |
|---|---|---|
| Output | Bottom panel | Prints, warnings and errors; click an error to open its line. |
| Console (editor) | Beside Output | Type Lua against the edit world, or the running game during Play. |
| Breakpoints | Click a line number in a script tab | Pauses Play at that line with the call stack and locals; **F5** continues. Editor Play only. |
| Developer Console | **F9** in Play and development runs of the player | Log, frame Stats and Commands (`help`, `clear`, `fps`, and games' own via `ConsoleService`). |
| Profiler | View > Panels > Profiler | **Game** (frame, CPU, GPU, memory, physics, audio, scripts, network), **Graphics** (draw calls, triangles, passes), **Memory** (by kind and class). |
| Overlays | Viewport > Overlays | Colliders, constraints, raycasts, sound emitters, navigation mesh, agent paths, culling. |
| View modes | Viewport > View Mode | Unlit, base colour, wireframe, normals, metalness, roughness, ambient occlusion, levels of detail, overdraw, culling bounds. |
| Asset Check | File > Check Assets | Missing files, broken references and import failures, with fixes. |
| Tests panel | View > Panels > Tests | Runs game tests and shows each result. |
| Several clients | Home tab, Game group | Play as 1 to 8 players with simulated latency and loss. |

## Game tests

Tests are `tests/**/*.spec.lua` files. Each runs on the server side of a fresh copy of the game, with no window, at a fixed 60 frames a second, and never writes the project.

| Global | Does |
|---|---|
| `describe(name, body)` | A group of tests. |
| `it(name, body)` | One test. |
| `beforeEach(f)`, `afterEach(f)` | Run around each test in the group. |
| `expect(value)` | Matchers: `toBe`, `toEqual`, `toBeNear`, `toBeTruthy`, `toBeGreaterThan`, `toBeLessThan`, `toThrow`, `toHaveAttribute`. |
| `Test.step(seconds)` | Runs the game on: physics, scripts, controllers, tweens, animation. |
| `Test.press(input, seconds)` | Holds a key, mouse or controller button, or input action. |
| `Test.click(button)`, `Test.type(box, text)` | Uses UI as a player would. |
| `Test.find(path)` | The Instance at a path such as `Workspace.House.Door`. |
| `Test.wait(condition, timeout)` | Runs the game until `condition()` is true, or fails after the timeout (5 s). |
| `Test.level(name)` | Runs the following tests in another level. |

```lua
describe("The character", function()
	it("walks forward when W is held", function()
		local body = Test.find("Workspace.Character.Body")
		local start = body.Position
		Test.press("W", 1)
		expect((body.Position - start).Magnitude).toBeGreaterThan(5)
	end)
end)
```

From a terminal: `axcenity-test <project folder> [--junit results.xml]` prints PASS or FAIL per test and exits with 0 when all passed, 1 when any failed. Built games never include `tests/`.

## Benchmarks

`axcenity-test <project> --bench 600` runs a project for 600 frames with no window and reports the time of each frame step; the player and server take `--bench` too. Engine developers use this to measure changes against saved baselines.

## Logs

The player writes `logs/latest.log` in the game's data folder and keeps the four runs before it. If the game crashes, the log ends with the reason, and the player says where the log is.

## AI assistants

`axcenity-editor --mcp` serves an open project to an AI assistant (Claude Code, Cursor, Codex, VS Code or a local model) over the Model Context Protocol: it can inspect the scene, make and change objects, write scripts, and run checks and a short Play. Every change it makes is one undo step. **Experimental**; `axcenity-editor --mcp --check` checks a setup.
