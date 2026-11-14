---
title: "ADR-0026: Convert new model formats to glTF on import"
type: adr
adr: 26
adr_status: Proposed
date: 2026-11-14
supersedes:
superseded_by:
affects: [assets, editor, player]
source: [crates/assets/src/model.rs, crates/assets/src/model/fbx.rs, crates/assets/src/import.rs]
docs: [docs/research/import-formats-and-plugins.md, docs/editor/import.md]
tags: [adr, asset-pipeline, import]
---

# ADR-0026: Convert new model formats to glTF on import

- **Status:** Proposed (P7 investigation, 2026-11-14; Paul's priority 7)
- **Date:** 2026-11-14
- **Affected systems:** model loading (`assets`), the editor's Import…, built games

## Context and problem

Models are read at load time, in the editor and in built games alike: glTF through `gltf`, FBX through `ufbx` (`crates/assets/src/model.rs`, `ModelFormat`). Paul asked for USD and Alembic to be investigated. The only pure-Rust USD reader, `openusd` 0.8 (MIT), is about 140k lines with its own dependencies; Alembic has no Rust reader at all (`docs/research/import-formats-and-plugins.md`). Everything downstream of loading (cooking, LOD and collision objects, `MeshId` object picks, the Import dialog, Model Studio, Edit in Blender) is written for glTF and FBX.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Read each new format at load time, as FBX is | The project keeps the user's own file; the watcher picks up DCC edits | The player and server compile and ship every reader; every downstream feature needs a path per format; load time pays for composition |
| **Convert to `.glb` in the editor when imported** | Only the editor links the reader; after import it's an ordinary `.glb`, so every downstream feature and built games work unchanged; one converter serves later formats | The project holds the converted copy; DCC changes come in by importing again; data our glTF reader doesn't use is lost unless converted on purpose |
| Ask users to export glTF themselves | Nothing to build | The friction priority 7 is about |

## Decision

A model format the runtime doesn't already read is converted to `.glb` by the editor-only `convert` crate before the Import dialog opens, and the sidecar notes the source (`"converted_from"`). Formats a runtime dependency already reads stay direct: OBJ through `ufbx`.

## Reasons

- Built games and the dedicated server stay as small and as quick to build as today.
- glTF is already the engine's richest path (Model Studio saves it, Blender round-trips it), so converting into it loses nothing those paths keep.
- Godot does the same for `.blend` and FBX, for the same reasons.

## Consequences

- USD layers, variants and live DCC links aren't kept: the import takes the composed stage as it is then. Re-importing replaces it.
- Animation conversion (UsdSkel) is its own stage (P7.6), since our glTF reader doesn't read glTF animation; it will need takes written beside the `.glb` or glTF animation support.
- Revisit if games need to load such files at run time (user-generated content), or if a format's data can't survive glTF.

## Implementation references

- `docs/research/import-formats-and-plugins.md` (D1), stages P7.1 (OBJ) and P7.2 (USD)
