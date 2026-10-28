# `testing`

- The game test runner (Phase 53): `Runner` lists and runs a project's `tests/**/*.spec.lua`, each `it` in a fresh headless `TestGame` (`game.rs`, the player's frame steps 1-4 at a fixed 1/60 s); `runner.lua` is `describe`/`it`/`expect`/`Test`, started by `ScriptRuntime::start_spec`. The `axcenity-test` binary is `main.rs`; JUnit XML `junit.rs`. Behavior: `docs/project/game-tests.md`.
- Never write the project's files: saves go to a temporary folder, and an older project isn't upgraded. The editor's Tests panel and the binary must both run through `Runner` so their results match.
- Keep `TestGame::frame` in the player's frame order when that changes. The player never depends on this crate.
