# Verification

Run these after implementing, before ticking criteria or committing. Start with the narrowest checks for the changed crates, then widen.

## Rust checks

From the repository root:

```text
cargo fmt --all
cargo nextest run -p <changed crate> [test name]
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo test --workspace --doc
```

If the change touched a `Cargo.toml` dependency, run `cargo hakari generate` first (and `cargo hakari manage-deps` for a new crate); CI fails otherwise. Clippy with `--all-targets` already checks that everything compiles, so a separate `cargo build` is only needed before running a binary. If `cargo-nextest` isn't installed, use `cargo test --workspace --no-fail-fast`. A build after a dependency or `Cargo.toml` change can run past the default 2-minute command timeout: give it a 10-minute timeout or run it in the background.

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
