# Axcenity Engine

Rust 3D game engine and editor; Luau scripting. How to run it: [README.md](README.md).

## Docs

The map is [docs/README.md](docs/README.md). Open that only to find the file for this task, then open **that one file**. Do not open a topic folder, a plan, or a PRD in full. A plan or a PRD is one phase or one story, found from its headings.

When behavior changes, update the topic file. Keep each crate `CLAUDE.md` under 10 lines: what the crate is, which topic file to open, and local constraints.

## Invariants

- `player` must not depend on `editor`, `egui` or `blocks`; it translates block scripts and graphs through `blocks-runtime`.
- `render` only consumes draw data; it must not depend on Instances or scripts.
- Instances live in a generational-ID arena; don't make an `Rc<RefCell>` tree.
- Blocks translate to Lua; there is one scripting runtime. Use Axcenity's Roblox-familiar API names, never Roblox branding.
- Keep the engine cross-platform. Pin new dependencies in the workspace and check Apache-2.0-compatible licensing.
- Follow the [development policy](docs/development-policy.md): desktop first (Windows and Linux), no infrastructure before it's needed, local tests for significant changes. GitHub Actions is off on purpose, so local runs are the only check.

## Commands

- Player: `cargo run -p axcenity-player -- examples/hello-part`
- Editor: `cargo run -p axcenity-editor -- examples/hello-touched`
- Game tests: `cargo run -p axcenity-testing -- examples/hello-character`
- Host and join: `cargo run -p axcenity-player -- --host examples/hello-part`, then `-- --join 127.0.0.1:47800 examples/hello-part`. Dedicated server: `cargo run -p axcenity-server -- examples/hello-part`
- Build: `cargo build --workspace`
- Test: `cargo nextest run --workspace` (one crate: `-p axcenity-x`). Doc tests: `cargo test --workspace --doc`
- After changing dependencies: `cargo hakari generate` ([docs/development.md](docs/development.md))
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all`

Prefer simple, idiomatic Rust. Briefly explain non-obvious Rust concepts when introducing them. Treat source and tests as authoritative when docs disagree, then update the topic file.
