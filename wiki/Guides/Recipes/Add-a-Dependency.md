---
title: Add a dependency
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [Cargo.toml, .config/hakari.toml]
docs: [docs/development.md, docs/architecture/crates.md, docs/development-policy.md]
tags: [guide, recipe, dependencies]
---

# Add a dependency

> [!summary] When to use this
> Adding a third-party crate, or a new engine-internal edge between crates.

## Before you start

- Policy: no infrastructure before it's needed; is the capability really missing? Check [Dependencies](../../Reference/Dependencies.md) for something already in the tree.
- For an engine edge, check [System dependencies](../../Architecture/System-Dependencies.md#forbidden-edges): no cycles, no forbidden edges. If two crates need each other, move the shared type to `core`.

## Steps

1. **License**: confirm Apache-2.0 compatibility (MIT, Apache-2.0, BSD, Zlib, CC0 have been accepted; note anything unusual in the comment).
2. **Pin it once** in `[workspace.dependencies]` of the root `Cargo.toml` with a comment saying what it's for and its license; turn off default features you don't need.
3. Use it from the crate with `name.workspace = true`.
4. If it's development-only, put it behind a crate feature nothing else can turn on (like the editor's `hot-patch`).
5. Run `cargo hakari generate` (and `cargo hakari manage-deps` for a new crate). Check `.config/hakari.toml`'s `final-excludes` if it must stay out of the player or server.
6. Confirm the player and server didn't gain it if they shouldn't: `cargo tree -p axcenity-server -e normal -i <crate>`, and the platform script's "player has no editor code" check.
7. Update `docs/architecture/crates.md` for a new engine edge, and [Dependencies](../../Reference/Dependencies.md).

## Pitfalls

- Coupled stacks move together: wgpu + winit + eframe/egui + egui_dock + egui-phosphor (M12); mlua + `luau0-src`; renet + renet_netcode.
- A crate that needs a C/C++ compiler is acceptable (Luau, ufbx, meshopt already do) but must build on Windows and Linux.
