---
title: Toolbox (online asset sources)
type: system
status: current
implementation: implemented
crates: [toolbox, editor]
source: [crates/toolbox/src/lib.rs, crates/toolbox/src/source.rs, crates/toolbox/src/record.rs, crates/toolbox/src/download.rs, crates/toolbox/src/db.rs, crates/toolbox/src/provenance.rs, crates/editor/src/toolbox_panel.rs]
docs: [docs/research/asset-sources/README.md, docs/research/asset-sources/provider-interface.md, docs/editor/assets.md, crates/toolbox/CLAUDE.md]
verified: 2026-11-11
verified_commit: 19c7179d
tags: [system/editor, assets, toolbox]
---

# Toolbox (online asset sources)

> [!summary]
> Roadmap **G1** (PRs #90, #92, #96, #107): the editor's one way to find and fetch assets online. The **`toolbox`** crate defines `AssetSource` and the shared `AssetRecord`, with **ambientCG** (v3 API), **Poly Haven** and **Openverse** sources; verified downloads (size, MD5, `.part` files, unpacking) into a **central asset directory** recorded in a SQLite `metadata.db` (`rusqlite`, versioned schema, also read from Python); a library (favourites, collections, recents); provenance (`.axcenity/asset-sources.json`, `CREDITS.txt`); Toolbox packs; the Blender library migration; the `axcenity-toolbox` CLI and a Python package with Blender providers. The editor's **Toolbox tab** (`toolbox_panel.rs`) replaced the Assets panel's old ambientCG "Get texture". **Editor only.** **Implemented.**

## Searching

Textures and skies search **All sources** by default: `search` in `toolbox_panel.rs` asks each source on its own thread (`std::thread::scope`), interleaves their pages, and reports a source that fails while others answered (`Found::failed`) instead of failing the search. Results are deduplicated by `provider:id` only; assets from two sources are never merged. The Sounds tab carries a licence notice (`SOUND_LICENCE_NOTICE`) above Openverse's results; each card keeps its own licence.

## Where its assets show

The Assets panel groups a project's assets by where they came from (`asset_browser/origin.rs`): each Toolbox source gets a section, read from the project's `.axcenity/asset-sources.json` (`Snapshot::source_of`), beside Built-in, Made in Axcenity, Imported and Global assets. A model's folder takes its model's origin, and a material made from a texture set is listed under that set.

## Flow

```mermaid
flowchart LR
  T[Toolbox tab / Ctrl+K /<br/>Terrain Studio From Toolbox] --> S["AssetSource: ambientCG,<br/>Poly Haven, Openverse"]
  S --> R[AssetRecords]
  R --> F["Fetcher: .part, size + MD5,<br/>allowed hosts, unpack"]
  F --> DIR["central asset directory<br/>+ metadata.db (SQLite)"]
  DIR --> P["copy into project assets/<br/>+ provenance, CREDITS.txt"]
```

## Rules (from `crates/toolbox/CLAUDE.md`)

- `player`, `server`, `sim` and `testing` never depend on it.
- Unit tests never touch the network (`http::Fixture`) or the real directory; `tests/live.rs` is run by hand after changing a source.
- `SourceError` messages are safe to show; every request sends `http::USER_AGENT`; never offer `.blend` files; check every download URL with `SourceInfo::allows_download_from`.
- Change the DB schema only through a new version in `db.rs`; never delete `assets` rows.

## Related documentation

`docs/research/asset-sources/` (assessment, architecture options, provider interface, experience); [Asset pipeline overview](../Asset-Pipeline/Pipeline-Overview.md); [Dependencies](../Reference/Dependencies.md)
