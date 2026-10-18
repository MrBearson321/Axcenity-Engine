# Axcenity Engine

Rust 3D game engine and editor; Lua 5.4 scripting. See [README.md](README.md) for the current project status and getting started.

## Load only relevant context

- Workspace members and dependency pins: [Cargo.toml](Cargo.toml).
- Crate boundaries, Instance model, frame loop, editor and Play mode: [docs/architecture.md](docs/architecture.md).
- Lua API, sandbox and scheduler: [docs/scripting.md](docs/scripting.md).
- Renderer and materials: [docs/rendering.md](docs/rendering.md).
- Project files, export plans and licensing: [docs/project-and-export.md](docs/project-and-export.md).
- In-game UI design: [docs/ui.md](docs/ui.md).
- Product scope and stories: [prd/axcenity-engine-v1.md](prd/axcenity-engine-v1.md).
- Roadmap and acceptance criteria: [plans/axcenity-engine-v1.md](plans/axcenity-engine-v1.md). Read only the relevant phase.

Open only the source, tests and doc sections needed for the task. For large docs, search headings first; don't load the whole PRD or plan, or copy their details into these instructions. Treat source and tests as authoritative when docs disagree, then update the relevant docs.

## Invariants

- `player` must not depend on `editor`, `egui` or `blocks`.
- `render` only consumes draw data; it must not depend on Instances or scripts.
- Instances live in a generational-ID arena; don't make an `Rc<RefCell>` tree.
- Blocks translate to Lua; there is one scripting runtime. Use Axcenity's Roblox-familiar API names, never Roblox branding.
- Keep the engine cross-platform. Pin new dependencies in the workspace and check Apache-2.0-compatible licensing.

## Commands

- Player: `cargo run -p axcenity-player -- examples/hello-part`
- Editor: `cargo run -p axcenity-editor -- examples/hello-touched`
- Build: `cargo build --workspace`
- Test: `cargo test --workspace`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all`

Prefer simple, idiomatic Rust. Briefly explain non-obvious Rust concepts when introducing them. When behavior or a design decision changes, update its canonical doc. Keep crate `CLAUDE.md` files under 10 lines: purpose, doc pointers and only non-obvious local constraints.
