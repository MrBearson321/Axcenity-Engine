# Game tests

A project's automated tests (unified Phase 53). Code: `crates/testing` (the runner, its Lua `src/runner.lua`, the headless `TestGame`, JUnit XML and the `axcenity-test` binary), `crates/script/src/spec.rs` (`ScriptRuntime::start_spec`, the runner's way into a game), `crates/editor/src/game_tests.rs` and `app/game_tests.rs` (the Tests panel). Example: `examples/hello-character/tests/walk.spec.lua`.

## Spec files

Tests are `tests/**/*.spec.lua` in the project folder. Each file runs on the server side, in an environment like a Script's, with these globals added:

| Global | Does |
|---|---|
| `describe(name, body)` | A group; `body` declares tests and groups inside it. Names nest: `Character > walks`. |
| `it(name, body)` | One test. |
| `beforeEach(f)`, `afterEach(f)` | Run around each test of their group and the groups inside: `beforeEach` outermost first, `afterEach` innermost first. `afterEach` runs even after a failure. |
| `expect(value)` | Matchers: `toBe`, `toEqual` (deep, for tables), `toBeNear(v, tolerance)` (numbers, Vector3s, Vector2s; 0.001 without), `toBeTruthy`, `toBeGreaterThan`, `toBeLessThan`, `toThrow(message?)` (a function that errors, with `message` in its error), `toHaveAttribute(name, value?)`, and `never` before any of them (`expect(x).never.toBe(y)`). `.` and `:` both work. |
| `Test.level(name)` | The tests declared after it in this group (and groups inside) run in that level; without it, the start level. |
| `Test.step(seconds?)` | Runs the game on for `seconds` (one frame without): controllers, agents, scripts, tweens, physics and Animators. |
| `Test.press(input, seconds?)` | Holds a key (`W`, `LeftShift`), controller button (`ButtonA`), mouse button (`MouseButton1`) or input action (`Jump`, its first input) for `seconds`, or one frame. |
| `Test.click(button)`, `Test.type(textBox, text)` | Click a TextButton as the player would (`Activated`), or set a TextBox's text and finish it (`FocusLost`); then one frame passes. |
| `Test.find(path)` | The Instance at `Workspace.Character.Body` (or with `/`), from `game`, services by name too; `nil` if there's none. |
| `Test.wait(condition, timeout?)` | Runs the game on until `condition()` is truthy and returns it; fails after `timeout` seconds (5 without). |

A failure reads `tests/walk.spec.lua:14: expected 0 to be greater than 5`: the spec line that called the matcher (`host.location`, the nearest script code on the stack), then what was compared. Any other error fails the test with its own `path:line:` message.

## How tests run

- The runner first runs a file's top level in a copy of the start level (without the game's scripts) to list its tests (`Runner::collect`). A file that doesn't compile, or whose top level errors or waits, is one failure with no test name.
- Each `it` then runs in a **fresh copy of its level** (`TestGame`): the project as loaded (`project::load`, or `load_with_level`), the starter character, Animators in their first states, then every script started as the player does, both sides. There's no window, audio or drawing. The spec runs as a Script of its own in ServerScriptService, named after the file, and its `task.wait`s are frames.
- Frames are a fixed 1/60 second and run the player's steps 1 to 4 (`docs/architecture/frame-loop.md`), then the render step's scripts and the game camera. What the spec asked for (`TestRequest`: press, click, type) is applied at the start of the next frame, so `Test.press("W", 1)` holds W for exactly 60 frames.
- A test ends when it reports, when its coroutine dies (the watchdog's error is its failure), or after 120 seconds of game time. An unknown input name fails it. A failure never stops the other tests.
- Tests never write the project: the project is read once, each test gets a copy of the world, and `SaveService` writes to a temporary folder deleted after the test. A project in an older format isn't upgraded; open it in the editor first.
- Exported games never include `tests/`: `project::pack_game` only packs scripts, assets, levels, UI graphs and the project's own files.

## Running them

- **Editor:** View > Panels > **Tests**. **Run all**, the play button on a file (run file) or a test (run one). The tree shows each file and test as not run, queued, passed or failed; a failure shows its `file:line` (opens the spec there) and message. Clicking a test's name opens it at its `it`. **Re-run on save** runs the last tests again whenever the editor saves (a script, a spec, or the project). Runs read the saved files, on a thread of their own. Spec tabs save as plain files; no Script is made for them.
- **Command line:** `axcenity-test <project> [--junit results.xml]` (`cargo run -p axcenity-testing -- examples/hello-character`). It prints `PASS`/`FAIL` per test, with failures' messages and output, then the counts. Exit code 0 when all passed, 1 when any failed, 2 when the project can't be opened or the arguments are wrong. `--junit` writes one `testsuite` per spec file for CI.

Both go through `axcenity_testing::Runner`, so they find the same tests and get the same results.
