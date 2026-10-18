# Axcenity Engine

A Roblox Studio-inspired 3D game engine and editor in Rust, scripted with Lua 5.4 and Scratch-style blocks. Targets Windows, macOS and Linux. Licensed Apache-2.0.

**Status:** Phase 13 (Script editing). Crates so far: `core`, `project`, `render`, `scene`, `physics`, `script`, `player`, `editor`.

## Read only what the task needs

| Working on… | Read |
|---|---|
| crate layout, dependencies, frame loop, Instance tree, Play mode | `docs/architecture.md` |
| `script` or `blocks` crates, Lua API, sandbox, scheduler | `docs/scripting.md` |
| `render` crate, materials, lights | `docs/rendering.md` |
| `physics` crate, Anchored, gravity, fixed step | `docs/architecture.md` (Physics) |
| `editor` or `scene` crates, panels, viewport camera | `docs/architecture.md` (Editor) |
| `ui` crate, CSS, layout | `docs/ui.md` |
| `project` or `player` crates, export, CI, licenses | `docs/project-and-export.md` |
| product scope, user stories, milestones, out of scope | `prd/axcenity-engine-v1.md` (large; grep for the section you need) |
| what to build next, acceptance criteria for a phase | `plans/axcenity-engine-v1.md` (grep `## Phase N`; read only the current phase) |

When a crate is created, give it its own `crates/<name>/CLAUDE.md` (under 20 lines: purpose, key types, gotchas). Those load automatically only when working in that crate, so keep crate-specific detail there, not here.

## Hard rules

- `player` never depends on `editor`, `egui` or `blocks`.
- `render` never sees Instances or scripts. It only receives a draw list.
- Instances live in an arena with generational IDs. No `Rc<RefCell>` trees.
- Blocks always translate to Lua. There is one scripting runtime.
- Lua API: Roblox-familiar names, never Roblox branding.
- New dependencies: pinned in `[workspace.dependencies]`, Apache-2.0-compatible license.
- Nothing may assume a single OS.

## Commands

- Run the player: `cargo run -p axcenity-player -- examples/hello-part`
- Run the editor: `cargo run -p axcenity-editor -- examples/hello-touched`
- Build: `cargo build --workspace`
- Test: `cargo test --workspace`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all`

## Working with the developer

The developer is new to Rust. Prefer simple, idiomatic code over clever code. Briefly explain non-obvious Rust (lifetimes, traits, borrow errors) when introducing it. For design choices, give options with trade-offs and one recommendation.

## Keeping docs lean

Update the relevant `docs/*.md` when a decision changes, and keep this file a short index. Don't copy doc content into it.
