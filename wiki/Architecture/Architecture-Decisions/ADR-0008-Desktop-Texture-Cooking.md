---
title: "ADR-0008: Cook assets offline into a per-machine folder, desktop formats"
type: adr
adr: 8
adr_status: Accepted
date: 2026-11-10
affects: [assets, cook, editor, render, project, player]
source: [crates/cook/src/lib.rs, crates/assets/src/cooked.rs, crates/assets/src/model/cooked_model.rs, crates/editor/src/cooking.rs]
docs: [docs/rendering/textures.md, docs/development-policy.md, docs/research/engine-modernization-report.md]
tags: [adr, assets]
---

# ADR-0008: Cook assets offline into a per-machine folder, desktop formats

- **Status:** Accepted (roadmap L2, 2026-11-10; format choice from the development policy)
- **Affected systems:** asset loading, texture upload, Build Game, the player

## Context and problem

Games parsed FBX/glTF, decoded JPEGs, built mipmaps on the CPU and uploaded RGBA8 at runtime: seconds of first-frame work and 4× the GPU memory. Mobile would need ASTC/ETC2; desktop has BC7/BC5 everywhere.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Keep runtime decoding | No pipeline | Slow startup, big VRAM |
| Godot-style `.import` files committed beside sources | Shared across machines | Large binary churn in git |
| **Cook in the editor into git-ignored `.axcenity/imported/`; ship current copies in builds** | Sources stay the truth; per-machine cache; stale copies detectable | Each machine cooks once; builds ship sources too |
| Multi-format (BC7 + ASTC + ETC2), KTX2 container | Mobile-ready | Policy: mobile deferred; extra code paths now |

## Decision

The editor cooks (BC7 mips via `intel_tex_2`, BC5 normal maps, simplified LODs, whole cooked models, embedded images in place) into `.axcenity/imported/`, named from the source path's hash or the image's bytes. A copy is used only while it matches its source (length + mtime, or content hash). Build Game packs current copies. Only `cook` links ISPC; the player only reads. Stable asset IDs live in committed sidecars; references still name paths.

## Consequences

- Big startup and VRAM wins (4096² map 85 → 21 MiB; Farmhouse 947 → 311 MB).
- Games ship sources plus copies (larger packages) until sources with copies can be dropped. *Update 2026-11-12 (G12):* Build Game now drops texture sources that have a current copy (empty stand-ins listed in `cooked-only.txt`; the CPU decodes the copy's top level when it needs pixels). Model sources still ship.
- lavapipe/no-BC devices fall back to RGBA8.
- Revisit formats when mobile is planned (policy).

## Implementation references

`crates/cook`, `crates/assets/src/cooked.rs`, `model/cooked_model.rs`, `model/lod_cache.rs`, `asset_id.rs`; [Cooking](../../Asset-Pipeline/Cooking.md).
