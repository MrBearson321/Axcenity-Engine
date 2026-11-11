---
title: Repository structure
type: architecture
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [Cargo.toml, rust-toolchain.toml, .config/nextest.toml, .config/hakari.toml]
docs: [docs/README.md, docs/architecture/crates.md]
tags: [architecture, repository]
---

# Repository structure

What each top-level folder holds and who reads it. Crate responsibilities are in [System inventory](System-Inventory.md); the file-splitting convention inside big crates is in [Coding standards](../Development/Coding-Standards.md#big-files).

## Top level

| Path | Holds | Notes |
|---|---|---|
| `crates/` | The 28 workspace crates (`members = ["crates/*"]`) | Each has a `CLAUDE.md` under 10 lines |
| `docs/` | Topic files: one short file per kind of change, plus `research/` | Map: `docs/README.md`. The reference for behavior |
| `docs/research/` | The 2026-11-10 modernization report and its working notes; the usability and design-system audits; the native-vector check | Evidence for the roadmap, not behavior docs |
| `plans/` | Phased roadmaps with acceptance checkboxes: `v1`, `v2`, `v3`, `graphics`, `unified` (current) | Open one phase by heading; never read whole |
| `prd/` | Product stories for v1 and v2 | One story at a time |
| `examples/` | 67 example projects (`hello-*`, `primitives`) | Rendered and run by player tests; `hello-graphics/reference.png` is the look reference |
| `templates/` | Genre templates for New Project (`arena`, `first-person`, `third-person`, `top-down`, `vehicle`, `shared`) | Embedded by `crates/project/build.rs` |
| `definitions/` | Generated luau-lsp definitions: `axcenity.d.luau`, `axcenity-plugin.d.luau` | Regenerate with `cargo run -p axcenity-script --bin lua-definitions` |
| `benchmarks/` | `baselines.json` (per-machine numbers for `axcenity-bench-check`) and `results/` (raw JSONL runs) | Commit with the change that moved them |
| `assets/` | The engine's own branding, fonts and textures | Not a project's `assets/` |
| `packaging/` | Installers and packages per OS, `stage.py`, release-notes template | Checked by `scripts/check-packages.py` |
| `scripts/` | `check-platform.sh`, `check-platform.ps1`, `check-packages.py` | The local stand-in for CI |
| `documentation/`, `gitbook-docs.yaml` | User documentation: the Learn chapters (`learn/`), reference pages (`reference/`) and the generated API pages (`api/`); the same files are the editor's Learn and Documentation pages and the GitBook site | Checked by the editor's `start::docs::tests`; `api/` regenerated with `cargo run -p axcenity-script --bin lua-definitions` (`docs/editor/learn-and-docs.md`) |
| `.claude/skills/` | Repo skills for agents: `implement`, `organize`, `research` | See [Contribution workflow](../Development/Contribution-Workflow.md) |
| `.config/` | `nextest.toml` (timeouts, GPU test group), `hakari.toml` | |
| `.github/workflows/` | `ci.yml`, `release.yml` | Disabled in repo settings on purpose |
| `.vscode/` | luau-lsp set-up for `examples/`, separate rust-analyzer target dir | |
| `Cargo.toml` | Workspace, every pinned dependency, build profiles | |
| `rust-toolchain.toml` | Rust 1.97.0 | Raise with the workflow pins and fix new lints together |
| `selene.toml`, `axcenity.yml` | selene lint config with the engine's globals | |
| `CLAUDE.md` | Agent instructions: docs map, invariants, commands | |
| `LICENSE`, `NOTICE` | Apache-2.0 | |

## Inside a crate

- `src/lib.rs` (or `main.rs`), and big modules as **a parent file plus a folder of the same name** whose children hold one responsibility each and start with `use super::*;`. The table of where the big files split is in `docs/architecture/crates.md`.
- Tests sit beside the code: `foo_tests.rs` or `tests.rs` next to `foo.rs`, or `tests/` for integration tests (`crates/server/tests/headless.rs`, `crates/render/src/renderer/tests/`).
- `CLAUDE.md`: what the crate is, which topic file to open, local constraints.

## Inside a project (what the engine reads)

| Path | What |
|---|---|
| `axcenity.json` | Manifest: name, format version, start level, loading screen, version |
| `world.json`, `levels/<name>.world.json` | Instances |
| `*.server.lua`, `*.client.lua`, module scripts | Luau |
| `assets/` | Models, textures, sounds, skies, materials, prefabs, data tables, animations, particles |
| `ui/*.css`, `ui/*.uigraph.json` | Stylesheets and UI Event Graphs |
| `tests/**/*.spec.lua` | Game tests |
| `input.json`, `strings.json` | Input map, string table |
| `.axcenity/` | Per-machine state: cooked copies (`imported/`), undo, version history, asset IDs. Git-ignored |

Full list: [File formats](../Reference/File-Formats.md).
