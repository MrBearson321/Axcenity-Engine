# Verification

Run these after implementing, before ticking criteria or committing. Start with the narrowest checks for the changed crates, then widen.

## Rust checks

From the repository root:

```text
cargo fmt --all
cargo test -p <changed crate> [test name]
cargo clippy -p <changed crate> --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Fix failures the change caused. Known failures that already fail on `main` (see memory and `git stash` + rerun if unsure) don't block a commit; name them in the handoff. Format only files this task touched if `cargo fmt --all` would rewrite unrelated files. Report what ran and its result, and don't describe a compile or test pass as proof of visual, cross-platform, or feel behavior.

## Writing tests for criteria

Prefer a test that drives the real path: the player host stepping a loaded example, a Lua script through the runtime, a block translated to Lua and run, a project saved and reloaded. Name the test after the behavior (`a_second_jump_in_the_air_does_nothing`) so the handoff can cite it against the criterion.

## Runtime and visual checks

When a criterion is visual, run the player/editor on the affected example and check logs. On Windows, the bundled helper captures a window:

```text
powershell -ExecutionPolicy Bypass -File .claude/skills/implement/scripts/screenshot.ps1 -Exe target/debug/axcenity-player.exe -Arguments examples/<name> -Out <scratchpad>/shot.png
```

Inspect the screenshot rather than treating file creation as verification. Behavior that needs held input over time or interactive editor use usually can't be shown by a screenshot; leave it for the user.

## Cross-platform CI

Pushing triggers CI. Check it with `gh run list --limit 3` only when the user asks about CI or cross-platform results.
