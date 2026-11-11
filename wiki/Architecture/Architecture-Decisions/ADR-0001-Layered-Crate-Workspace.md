---
title: "ADR-0001: Layered crate workspace with forbidden edges"
type: adr
adr: 1
adr_status: Accepted
date: 2026-10-17
affects: [all crates]
source: [Cargo.toml, scripts/check-platform.sh, scripts/check-platform.ps1, .config/hakari.toml]
docs: [docs/architecture/crates.md, plans/axcenity-engine-v1.md]
tags: [adr]
---

# ADR-0001: Layered crate workspace with forbidden edges

- **Status:** Accepted (v1 plan, 2026-10-17; extended by every later plan and roadmap M8)
- **Affected systems:** every crate, the player, the server

## Context and problem

An engine and editor in one repository can easily let editor code leak into shipped games, let the renderer reach into gameplay state, and grow dependency cycles that make every rebuild touch everything. Games ship a player binary that should be small, and later a headless server that must start with no display.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| One crate | Simple | Player ships the editor; no enforced boundaries; slow incremental builds |
| A few big crates (engine, editor) | Fewer manifests | Boundaries inside crates are only conventions |
| **Many small crates with explicit forbidden edges** | Boundaries enforced by Cargo; small player and server; parallel builds | More manifests; shared types must find a home (`core`) |

## Decision

One Cargo workspace (`crates/*`) of focused crates, created in the first phase that needs one. Shared types live in `core`. Forbidden edges: `player` never depends on `editor`, `egui`, `blocks`, `plugins` or `cook`; `render` never on Instances or scripts; no cycles; the server links no window, GPU or audio. Third-party versions are pinned once in the workspace, Apache-2.0-compatible.

## Reasons

Cargo enforces what a convention can't. The player-has-no-editor-code rule keeps games small and the editor free to use heavy dependencies (egui, ureq, ISPC).

## Consequences

- New shared plain types go into `core` (input map, sound actions, splines, streaming partitions, ability slots) or a small types crate (`render-types`, M8).
- `workspace-hack` (cargo-hakari) is needed so `-p` builds reuse compiled dependencies.
- `editor` grew to 168k lines in one crate; L6 plans sub-crates behind an `EditorContext`.

## Implementation references

`Cargo.toml` `[workspace.dependencies]`; `scripts/check-platform.*` ("The player has no editor code"); `cargo tree -p axcenity-server -e normal -i wgpu`; `docs/architecture/crates.md`; [System dependencies](../System-Dependencies.md).
