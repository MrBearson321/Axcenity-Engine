---
title: Testing strategy
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [.config/nextest.toml, crates/testing/src/game.rs, crates/testing/src/runner.lua, crates/player/src/tests.rs, scripts/check-platform.ps1]
docs: [docs/development.md, docs/project/game-tests.md, docs/development-policy.md, .claude/skills/implement/references/verification.md]
tags: [development, testing]
---

# Testing strategy

Testing is **local**: GitHub Actions runs only for release tags and CI only by hand ([ADR-0011](../Architecture/Architecture-Decisions/ADR-0011-Desktop-First-Local-Verification.md), [ADR-0018](../Architecture/Architecture-Decisions/ADR-0018-Actions-For-Release-Tags-Only.md)). A significant change to `core`, the frame schedule, physics, scripting, asset loading, rendering, networking, or anything a game can observe adds or updates tests in the same change and runs them. The pull request says what ran and on which OS.

## Layers of tests

| Layer | Where | Runs with |
|---|---|---|
| Unit tests | `#[test]` beside the code (`foo_tests.rs`, `tests.rs`) in every crate; about 3,150 | `cargo nextest run --workspace` |
| Real-path integration | Player tests stepping loaded examples (`crates/player/src/*_tests.rs`), scripts through the real runtime, block scripts translated and run, projects saved and reloaded | same |
| GPU and image tests | `render` and `player` (`gpu` test group, max 8 at once); `every_example_renders` compares `hello-graphics` with `reference.png` per preset | same (needs a GPU) |
| Editor drawing tests | egui harness with `theme::test_context()`; PNGs to `AXCENITY_SNAPSHOTS` for inspection | same |
| Golden and drift tests | Frame order (`sim/src/golden.rs`), definitions file up to date, frame-loop doc table, example round trips | same |
| Game tests | `tests/**/*.spec.lua` in projects, each `it` in a fresh headless `TestGame` | `axcenity-test <project>` (JUnit with `--junit`), editor Tests panel |
| Headless server | `crates/server/tests/headless.rs` runs the real binary | nextest; `check-platform` with no display |
| Benchmarks and micro-benchmarks | `#[ignore]` tests and `--bench` | [Benchmarking](../Performance/Benchmarking.md) |
| Platform check | Everything above plus clippy, doc tests, `axcenity-test examples/hello-character`, headless server | `scripts/check-platform.sh` / `.ps1` |
| Packages | Installers, Build Game per OS | `python3 scripts/check-packages.py` |
| By hand | LAN between two computers; feel and visuals | Documented per phase in the plans' "Open checks" |

## The order to run checks

```text
cargo fmt --all
cargo nextest run -p <changed crate> [test name]
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace          # add --no-fail-fast with cargo test
cargo test --workspace --doc
```

After a dependency change, `cargo hakari generate` first. After a script API change, regenerate definitions (`cargo run -p axcenity-script --bin lua-definitions`).

## Writing good tests

- Drive the **real path** (the host stepping a loaded example, Lua through the runtime) rather than a mock.
- Name the test after the behavior, so a plan criterion can cite it.
- For caches and fast paths, prove equivalence with the slow path (randomized where possible).
- For parallel code, prove the output doesn't depend on thread count.
- Never pass `std::env::temp_dir()` itself to `Assets::new` in a test (it scans the whole temp folder); use a fresh subfolder.
- Keep single tests short; nextest marks a test SLOW after 30 s and kills it after 2 minutes.

## Known noise

Some tests are flaky under GPU contention (`every_example_renders` occasionally returns a black frame and passes on rerun) and a few editor tests are timing-sensitive under full load. The current list lives in the project memory note on test noise; treat any other failure as caused by your change until shown otherwise. Plan criteria are ticked only when a passing test or an inspected screenshot proves them.
