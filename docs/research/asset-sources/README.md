# Asset sources: one provider layer for the engine, the Toolbox and the Blender pipeline

Phase 3 of the engine modernization brief: how free asset catalogs (ambientCG, Poly Haven, Kenney, and later sound sources) should reach the engine, and how the three code bases that already talk to them fit together. This is a proposal. Nothing here is built yet, and the decisions at the bottom need Paul's sign-off before implementation starts.

- **Date:** 2026-11-11
- **Engine:** `main` at `2c030e29`.
- **Axcenity Toolbox:** `Documents\CIDM3312\project-two-building-with-apis` at `5b17c1d`. Read only; nothing was changed or run.
- **Blender pipeline ("3D Models"):** `Desktop\3D Models` at `6bf698d`. Read only; nothing was changed or run.

## The files

| File | What it answers |
|---|---|
| [toolbox-assessment.md](toolbox-assessment.md) | What the Toolbox, the Blender pipeline's provider layer and the engine's Get texture each do, how good they are, what to reuse and what must not cross over |
| [architecture-options.md](architecture-options.md) | Embedded, companion service, remote backend, or hybrid: tradeoffs and the recommendation |
| [provider-interface.md](provider-interface.md) | The one asset record, the Rust provider interface, the shared library on disk, the engine touch points, and the step-by-step migration |

## Summary

**Three provider layers exist today, in two languages.**

| Code base | Language | Sources | What it does with them |
|---|---|---|---|
| Engine editor, Get texture (`crates/editor/src/ambient_cg.rs`) | Rust | ambientCG (v2 API) | Search materials, download one zip, save a texture set into the project |
| Axcenity Toolbox | Python, Django | ambientCG (v3), Poly Haven, Openverse | Browse, save to a library, build packs, export an `axcenity-pack` manifest. Never downloads files itself |
| Blender pipeline (`tools/blender/providers/`, `tools/library.py`) | Python, no `bpy` needed | ambientCG (v2), Poly Haven, Kenney, Sketchfab (token), local folders, JSON catalogues; Freesound in one pack script | Download into a shared on-disk library (`asset_library/` with `registry.json`), verified by size and MD5, cached for offline use, with licence checks and credits |

Each has its own asset record. The engine has **no Poly Haven code** and fetches no HDRIs, models or sounds.

**Recommendation: one Rust provider layer, one record, one library on disk.**

1. **One record.** A versioned `asset-source` record that is the union of the Blender pipeline's `AssetRecord` and the Toolbox's `AssetRecord` (the two are close already), with normalised licence codes. Every code base reads and writes it; shared JSON fixtures keep them honest.
2. **One provider layer, in Rust.** A new crate, `asset-sources`, holds the providers (ambientCG on its v3 API, Poly Haven, Kenney, later Sketchfab and sound sources), HTTP, caching, verified downloads, licence rules and credits. It is embedded in the editor, and also builds a small command-line tool, `axcenity-assets`, that prints JSON, so the Blender pipeline can call the same code instead of keeping its own providers.
3. **One library on disk.** The Blender pipeline's `asset_library/` layout and `registry.json` become the shared, per-machine download library. The editor fetches into it and copies from it into a project (as Global Assets already copy), so a material downloaded once is there for Blender and the editor, online or off.
4. **The Toolbox stays a web companion.** It keeps its own browse-only Python clients (they never download files, and a web request is the wrong place to shell out to a CLI), but emits the shared record in its pack manifest, and the editor imports that manifest from a file.

No local Python service and no hosted backend, which follows the [development policy](../../development-policy.md): desktop first, no infrastructure before it's needed, and extension points (the trait, the record, the CLI's JSON) that cost little now.

## Decisions for Paul

1. **Architecture.** Embedded in the editor, with the Toolbox linked by its manifest file (recommended), rather than a local companion service, a hosted backend, or a live link to the Toolbox's API. See [architecture-options.md](architecture-options.md).
2. **Rust as the one implementation.** The Blender pipeline moves its provider calls onto the `axcenity-assets` CLI, source by source, and its Python providers are retired once each passes the same fixtures (recommended). The alternative keeps two implementations (Python for Blender and the Toolbox, Rust for the engine) held together only by the shared record and fixtures.
3. **Porting your code into the Apache-2.0 engine.** Neither the Toolbox nor the 3D Models repository has a licence file. You wrote both, so you can relicense, but porting their logic into the engine should be your explicit choice. Recommended: yes, for the provider, record, variant, licence and download-safety logic.
4. **Where the shared library lives.** One per-machine folder that the editor and the Blender pipeline both point at (recommended: an editor setting, defaulting to the editor's data folder, with `PIPELINE_ASSET_LIBRARY` pointing the pipeline at the same place). Today the pipeline's library sits inside the 3D Models folder.
5. **Provenance in the project.** Each file the editor installs gets an optional `source` block in its `*.import.json` sidecar (provider, id, licence, authors, page), so the editor can list credits and Build Game can write a `CREDITS.txt` for assets that need attribution (recommended; the alternative is one `.axcenity/sources.json`).

The migration's first step (moving the HTTP and zip helpers out of the Get texture dialog into the new crate, with no behaviour change) is safe to start before these are answered, since every option needs it.
