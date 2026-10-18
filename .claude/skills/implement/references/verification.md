# Opt-in verification

Load this reference only when the user asks for tests, builds, or other verification. Select the smallest checks that directly cover the requested behavior; do not run every command below automatically.

## Rust checks

From the repository root:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Use targeted crate or test selectors where possible. Run formatting separately if it needs to modify files. Report what was and was not run, and don't describe a compile or test pass as proof of visual, cross-platform, or user-experience behavior.

## Runtime and visual checks

When requested and relevant, run the player/editor on the affected example and check logs. For visual behavior on Windows, the bundled helper can capture a window:

```text
powershell -ExecutionPolicy Bypass -File .claude/skills/implement/scripts/screenshot.ps1 -Exe target/debug/axcenity-player.exe -Arguments examples/<name> -Out <scratchpad>/shot.png
```

Inspect the screenshot rather than treating file creation as verification. On other platforms, explain any checks that require the user's environment.

## Cross-platform CI

Check GitHub Actions only when the user asks for CI/cross-platform verification or the requested outcome depends on it. Use `gh run list --limit 3`, then inspect relevant runs and per-OS jobs. Don't push code or trigger CI unless asked.
