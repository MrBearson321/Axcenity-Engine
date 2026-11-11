---
title: Build and run
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [Cargo.toml, rust-toolchain.toml, .config/nextest.toml, .config/hakari.toml, scripts/check-platform.ps1, scripts/check-platform.sh]
docs: [README.md, docs/development.md, CLAUDE.md]
tags: [guide, build]
---

# Build and run

The commands, tools and settings for working on the engine. The detailed reference is `docs/development.md`; this page is the summary with the reasons.

## Set up once

- Rust **1.97.0** (pinned in `rust-toolchain.toml`; rustup picks it up), a C/C++ compiler (Luau, ufbx and meshoptimizer build from source with `cc`), and on Linux `libudev-dev` (gamepads) and ALSA headers (audio).
- `cargo install cargo-nextest --locked` and `cargo install cargo-hakari --locked`.
- Optional: Tracy (profiler), `luau-lsp` and `selene` (script tooling), Python 3 (package checks).

## Everyday commands

| Task | Command |
|---|---|
| Run the editor | `cargo run -p axcenity-editor -- examples/hello-touched` |
| Run the player | `cargo run -p axcenity-player -- examples/hello-part` |
| Host / join | `cargo run -p axcenity-player -- --host examples/hello-part`, then `-- --join 127.0.0.1:47800 examples/hello-part` |
| Dedicated server | `cargo run -p axcenity-server -- examples/hello-part` |
| Game tests | `cargo run -p axcenity-testing -- examples/hello-character` |
| Build everything | `cargo build --workspace` |
| All tests | `cargo nextest run --workspace` (about 25 s warm) |
| One crate and its dependents | `cargo nextest run --workspace -E 'rdeps(axcenity-core)'` |
| Doc tests | `cargo test --workspace --doc` (nextest skips them) |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Format | `cargo fmt --all` |
| After changing dependencies | `cargo hakari generate` (and `cargo hakari manage-deps` for a new crate) |
| Regenerate Luau definitions after an API change | `cargo run -p axcenity-script --bin lua-definitions` |
| Cook a project from the command line | `cargo run -p axcenity-editor --release -- --cook <project>` |
| Whole-platform check | `scripts/check-platform.sh` or `powershell -ExecutionPolicy Bypass -File scripts\check-platform.ps1` |
| Packages and Build Game | `python3 scripts/check-packages.py` |
| Benchmarks | [Benchmarking](../Performance/Benchmarking.md) |
| Engine version and release checks | `cargo run -p axcenity-release -- check` ([Release process](../Development/Release-Process.md)) |
| MCP server for AI clients | `axcenity-editor --mcp <project>` ([MCP server](../Editor/MCP-Server.md)) |

## Why the build is set up this way

| Setting | Reason | Source |
|---|---|---|
| `workspace-hack` (cargo-hakari) | Without it, `-p` builds compiled second copies of shared libraries: `cargo test -p axcenity-script --no-run` 2 min 20 s → 2 s; target folder 23 GB → 9 GB | `docs/development.md` |
| `[profile.dev.package."*"]` optimized, line tables only | Debug builds decode JPEGs and mipmaps in reasonable time; editor binary 1 GB → 550 MB; cold build peak RAM 3.9 → 2.4 GB | `Cargo.toml`, `docs/development.md` |
| `render` and `nav` optimized in dev | Hot paths unusable unoptimized | `Cargo.toml` |
| `--release`: thin LTO, line tables; `--profile dist`: fat LTO, one codegen unit, stripped | Fast shipped binaries; panics still name lines in release | I5 |
| `panic = "unwind"` everywhere | Asset loaders and editor Play recover with `catch_unwind` | `Cargo.toml` |
| Pinned toolchain | Clippy `-D warnings` would otherwise break on new lints with no code change | I5 |
| nextest GPU group (max 8) | Dozens of GPU tests at once timed wgpu out | `.config/nextest.toml` |
| rust-analyzer's own target dir | Its checks don't hold the build lock | `.vscode/settings.json` |

## Verification is local

GitHub Actions runs only for release tags, and CI only when started by hand ([ADR-0011](../Architecture/Architecture-Decisions/ADR-0011-Desktop-First-Local-Verification.md), [ADR-0018](../Architecture/Architecture-Decisions/ADR-0018-Actions-For-Release-Tags-Only.md)). A significant change adds tests and runs them on a developer machine, and the pull request says what ran and on which OS. See [Testing strategy](../Development/Testing-Strategy.md).

## When the build misbehaves

See [Troubleshooting](../Guides/Troubleshooting.md): "Blocking waiting for file lock", out-of-memory during a cold build (`cargo build -j 4`), GPU test timeouts, a Luau pin test failure.
