---
title: "ADR-0012: The wiki explains and links; topic docs stay the per-change reference"
type: adr
adr: 12
adr_status: Accepted
date: 2026-11-10
affects: [documentation]
docs: [CLAUDE.md, docs/README.md]
tags: [adr, documentation]
---

# ADR-0012: The wiki explains and links; topic docs stay the per-change reference

- **Status:** Accepted (2026-11-10, when this wiki was created)
- **Affected systems:** documentation, agent instructions

## Context and problem

The repository already had about 100 short `docs/` topic files and 28 crate `CLAUDE.md` files, kept in step with every change, written so an agent opens exactly one file per task. Paul asked for an Obsidian engineering wiki with architecture, workflows, decisions and performance history. Copying the topic files into the wiki would create two sources of truth that drift.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Move `docs/` into the vault | One place | Breaks the one-file-per-change convention, the skills and the `CLAUDE.md` map |
| Copy content into the wiki | Self-contained wiki | Duplication; drift |
| **Wiki as an explanatory layer that links to topic docs and source** | No duplication; each layer has one job | Readers follow links into `docs/` for detail |

## Decision

- `docs/` topic files and crate `CLAUDE.md` files remain the reference for behavior, kept up to date with each change as before.
- The wiki holds what they don't: architecture overviews and diagrams, traced workflows, ADRs, the optimization log and benchmark history, limitations and debt, guides.
- The wiki names repository files by path in backticks (checked by `scripts/check-wiki.py`) rather than linking outside the vault.
- Source and tests outrank both.

## Consequences

- A behavior change updates the topic file (as before) and, when it changes architecture, a decision, performance or a limitation, the wiki page in the same pull request.
- The validator flags pages whose sources changed since they were verified.

## Implementation references

[Wiki guide](../../Getting-Started/Wiki-Guide.md); `scripts/check-wiki.py`.
