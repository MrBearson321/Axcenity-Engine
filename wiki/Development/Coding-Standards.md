---
title: Coding standards
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [Cargo.toml, rust-toolchain.toml]
docs: [CLAUDE.md, docs/architecture/crates.md, docs/development.md]
tags: [development, standards]
---

# Coding standards

The conventions the codebase already follows. Where they come from: the root `CLAUDE.md`, `docs/architecture/crates.md`, crate `CLAUDE.md` files, and `Cargo.toml`'s lints.

## Rust

- **Simple, idiomatic Rust.** Explain non-obvious Rust concepts briefly where you introduce them (root `CLAUDE.md`).
- **Edition 2024**, Rust **1.97.0** (pinned). `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings` must be clean (`clippy::all` warns at the workspace level, `-D` turns warnings into errors).
- **`unsafe_code = "deny"`** for the whole workspace. The only allowed `unsafe` is in `crates/script/src/debugger.rs` (Luau's step hook and reading a paused script's locals). Keep new code safe.
- **No panics on bad input.** Loaders report errors once and carry on (`ModelError::Damaged`, `Project::problems`). `panic = "unwind"` stays on in every profile because asset loaders and editor Play recover with `catch_unwind`.
- **Errors**: `anyhow` in binaries and at the edges; typed errors where callers match on them. Script-facing errors read `path:line: message`.
- **No `HashMap` iteration into output.** Sort, or use `BTreeMap`, where order shows. `FxHashMap` for engine-keyed maps; SipHash for strings from players, files or the network.
- **Determinism where it's free**: seeded PRNGs inside the crate (never `rand` in generators), fixed-order parallel joins.
- **Print through the host's path**: the player uses `logs::line`, not `println!`; the editor's Output/Console; `log` macros elsewhere.

## Big files

A large module is **a parent file plus a folder of the same name**. The parent keeps the types and entry points; each child holds one responsibility (another `impl` block or free functions) and starts with `use super::*;`. Tests go in a `tests.rs` beside the parent (or `foo_tests.rs`). Library crates re-export moved items with `pub use` so paths don't change. The current split map is in `docs/architecture/crates.md` ("Where the big files split"). The `organize` skill treats ~1,500+ lines with several responsibilities as a split candidate; split by moving code verbatim, never "improving" it in the same step.

## Naming

- Rust: standard naming. Test names describe the behavior in a sentence (`a_second_jump_in_the_air_does_nothing`).
- Script API: Roblox-familiar names (`Instance.new`, `Touched:Connect`, `task.wait`), PascalCase classes and properties, methods with `:`. **Never Roblox branding** (no `RBX` names, logos or assets). Clean up Roblox quirks when it helps beginners; don't chase full compatibility.
- Writing in docs and UI: plain, short sentences; say what something does.

## Dependencies

Pin every third-party crate once in `[workspace.dependencies]` with an explaining comment (what it's for, its license); check Apache-2.0 compatibility; run `cargo hakari generate`. Development-only dependencies sit behind a feature nothing else can turn on (the editor's `hot-patch`). See [Add a dependency](../Guides/Recipes/Add-a-Dependency.md).

## Crate rules

Each crate's `CLAUDE.md` (under 10 lines) states what it is, which topic file to open, and its local constraints. Respect the forbidden edges in [System dependencies](../Architecture/System-Dependencies.md#forbidden-edges).

## Editor UI

Colors from `theme::palette(ctx)` tokens, controls from `widgets.rs`, layouts from `responsive.rs` (all in `editor-kit`) (fit a 260-point column); a `Command` for every menu item and ribbon button; route color pickers through `color_palette::edit_srgb`. See `crates/editor/CLAUDE.md` and the design-system audit.
