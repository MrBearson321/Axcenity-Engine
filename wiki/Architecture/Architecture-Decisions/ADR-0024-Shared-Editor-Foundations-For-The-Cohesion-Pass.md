---
title: "ADR-0024: Shared editor foundations for the cohesion pass"
type: adr
adr: 24
adr_status: Accepted
date: 2026-11-12
supersedes:
superseded_by:
affects: [editor-kit, editor, editor-tools, blocks-runtime, physics, collab]
source: [crates/editor-kit/src/theme.rs, crates/editor-kit/src/widgets.rs, crates/blocks-runtime/src/lib.rs, crates/blocks-runtime/src/node_graph.rs, crates/editor-tools/src/preview.rs, crates/physics/src/lib.rs]
docs: [docs/research/cohesion-pass.md]
tags: [adr, editor, design-system, visual-scripting, physics, collaborate]
---

# ADR-0024: Shared editor foundations for the cohesion pass

- **Status:** Accepted (from Paul's brief of 2026-11-12; the audit is `docs/research/cohesion-pass.md`)
- **Date:** 2026-11-12
- **Affected systems:** editor UI kit, editor documents and navigation, visual scripting, previews, physics, Collaborate

## Context and problem

The brief asks for five shared foundations (design system, visual scripting IR, physics abstraction, collaboration layers, preview pipeline) before features. The audit found each one already partly built: `editor-kit` tokens and widgets; `BlockScript` behind Blocks, Graph and Lua, and `NodeGraph` behind UI Event Graphs, behavior trees and dialogues; rapier confined to the `physics` crate; Collaborate's real-time, transport, sync and Git parts in `collab` and `repo` (ADR-0020–0022); `editor-tools/src/preview.rs` over `render::TextureRenderer`. What's missing is the pieces that join them: a menu button and category palette in the kit, one document tracker and one open action in the editor, one node catalogue, a module split inside `physics`, and the trust and firewall layer in Collaborate.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| New shared crates for each foundation | Clean slate | Duplicates working systems; breaks the "extend, don't parallel" rule; large churn |
| One merged graph format for Blocks and node graphs | One file type | Breaks saved files; the two model different things (statements with a lossless Lua round trip versus free wiring); nothing gained over a shared catalogue |
| A physics backend trait with dynamic dispatch | Swappable backend now | No second backend is planned (policy: no infrastructure before needed); costs indirection in the hottest loop |
| **Extend what exists, add the joining pieces** | Small, testable steps; saved files unchanged | Each foundation is finished in its own phase, not all at once |

## Decision

Extend the existing systems: the kit gains the missing controls and the category palette; the editor gains one `DocumentTracker` and one open action; `blocks-runtime` keeps `BlockScript` and `NodeGraph` as its two IRs and gains one node catalogue and one diagnostic type; `physics` keeps rapier behind a backend-neutral `api` module boundary (not a trait); Collaborate keeps its layers and gains a trust store, connection approval, narrower listening and firewall detection; previews build on `preview::Canvas`.

## Reasons

Every foundation already has users and tests. Replacing them would risk regressions for no new capability, and the development policy prefers extending systems to building parallel ones.

## Consequences

- New editors register a document with the tracker and an `OpenTarget`; new controls come from the kit.
- A new block or node kind needs a catalogue entry (a test fails otherwise).
- A second physics backend later needs a trait at the `api` boundary; nothing outside `physics` changes.
- Revisit the two-IR decision if a block-to-node-graph conversion is ever asked for.

## Implementation references

- Plan and findings CF1–CF22: `docs/research/cohesion-pass.md`
- Phases CP2–CP9, one PR each
