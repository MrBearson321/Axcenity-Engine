# Axcenity Toolbox in the engine: assessment, architecture and plan

Phase 3 of Paul's feature-expansion brief (2026-11-11): make the Axcenity Toolbox the engine's one asset discovery and retrieval layer, **Engine → Toolbox → Providers**, replacing the engine's own ambientCG code and adding Poly Haven, with a Roblox-style Toolbox inside the editor. It also brings Paul's Blender pipeline, which has a third set of provider code, onto the same layer. This is a proposal. Nothing here is built yet, and the decisions at the bottom need Paul's sign-off before implementation starts.

- **Date:** 2026-11-11
- **Engine:** `main` at `2c030e29`.
- **Axcenity Toolbox (Django):** `Documents\CIDM3312\project-two-building-with-apis` at `5b17c1d`. Read only; nothing was changed or run.
- **Blender pipeline ("3D Models"):** `Desktop\3D Models` at `6bf698d`. Read only; nothing was changed or run.

## The files

| File | What it answers |
|---|---|
| [toolbox-assessment.md](toolbox-assessment.md) | The Django Toolbox examined area by area (brief 3.1), plus the Blender pipeline's providers and the engine's Get texture: what to reuse, modify or redesign, and what must not cross over |
| [architecture-options.md](architecture-options.md) | Embedded module, local companion service, separate backend, or hybrid (brief 3.2): tradeoffs and the recommendation |
| [provider-interface.md](provider-interface.md) | The interface between the engine and the Toolbox, the one asset record, the provider trait, the shared library on disk, engine touch points and the migration plan |
| [toolbox-experience.md](toolbox-experience.md) | The Roblox-style Toolbox panel and how it plugs into the Assets panel, Explorer, Material tab, Terrain Studio, model import, audio and project files (brief 3.3 and 3.4) |

## Summary

**Three provider layers exist today, in two languages.**

| Code base | Language | Sources | What it does with them |
|---|---|---|---|
| Engine editor, Get texture (`crates/editor/src/ambient_cg.rs`) | Rust | ambientCG (v2 API) | Search materials, download one zip, save a texture set into the project |
| Axcenity Toolbox | Python, Django | ambientCG (v3), Poly Haven, Openverse | Browse, save to a library, build packs, export an `axcenity-pack` manifest. Never downloads files itself |
| Blender pipeline (`tools/blender/providers/`, `tools/library.py`) | Python, no `bpy` needed | ambientCG (v2), Poly Haven, Kenney, Sketchfab (token), local folders, JSON catalogues; Freesound in one pack script | Download into a shared on-disk library (`asset_library/` with `registry.json`), verified by size and MD5, cached for offline use, with licence checks and credits |

The engine has **no Poly Haven code** today (the brief assumed it had); it fetches no HDRIs, models or sounds.

**Recommendation: keep the brief's layering, but make the Toolbox a native module, not a web service.**

```
 Editor panels (Toolbox, Assets, Explorer, Material tab, Terrain Studio, Import)
        │  one interface: search, browse, record, preview, fetch, install, collections
        ▼
 Axcenity Toolbox  (Rust crate `toolbox`, in the editor process)
   record · provider trait · cache · verified downloads · licences · shared library
        │                                    ▲
        ▼                                    │ same code, through the `axcenity-toolbox` CLI
 Providers: ambientCG · Poly Haven · Openverse (sounds) · later Kenney, Sketchfab…     Blender pipeline
                                             ▲
 Django Toolbox site ── exports axcenity-pack manifests ── imported by the editor
```

1. **The Toolbox becomes a Rust crate, `toolbox`,** that the editor embeds. The editor talks only to its interface; ambientCG, Poly Haven and every later source sit behind one provider trait. That is the brief's "Engine → Toolbox → Providers", without a network hop or a second runtime.
2. **The Django app's logic is extracted, not hosted.** Its provider clients, record, variant choice, licence engine and manifest are ported to the crate. The Django site stays an optional web companion for browsing and sharing packs; the editor imports its pack files.
3. **One record and one library on disk** for the engine and the Blender pipeline. The pipeline calls the same Rust code through a small CLI, and both use one per-machine download library (the pipeline's existing layout), so an asset downloaded once is there for both, online or off.
4. **A Toolbox panel in the editor** for searching, browsing, previewing, favourites, collections, recents and download history, which installs through the editor's existing paths (texture sets, materials, skies, Import, Sounds) rather than a parallel workflow.

No local Python service and no hosted backend, which follows the [development policy](../../development-policy.md): desktop first, no infrastructure before it's needed, and extension points (the trait, the record, the CLI's JSON, the manifest) that cost little now.

## Paul's decisions (2026-11-11)

Paul answered: "Build it into the editor with a shared backend. Use a rust core and a python blender integration. Conditional migration with a centralized axcenity asset directory. Make the source and license records persistent per-asset metadata database." Read as follows; where a reading was a judgement call it says so.

| # | Decision | What it means for the design |
|---|---|---|
| 1 | **Built into the editor, with a shared backend** | The Toolbox is the `toolbox` Rust crate inside the editor. "Shared backend" is read as the same crate, the same asset directory and the same metadata database serving both the editor and Blender, not a hosted server |
| 2 | **Rust core, Python Blender integration** | The providers, downloads and database live only in Rust. The Blender side is a thin Python package that runs the `axcenity-toolbox` CLI and reads the metadata database with Python's built-in `sqlite3` |
| 3 | **Conditional migration** | Judgement call, read as two conditions: (a) each of the Blender pipeline's Python providers is retired only once the Rust source passes the same fixtures and its existing tests; (b) the pipeline's existing `asset_library/` is imported into the central directory only entry by entry, and only where its files still verify (size, MD5), leaving the old folder untouched. Porting Paul's Toolbox and 3D Models logic into the Apache-2.0 engine is taken as approved by the choice of a Rust core |
| 4 | **A centralized Axcenity asset directory** | One per-user directory for downloaded originals, previews, the API cache and the database: `%LOCALAPPDATA%\Axcenity\Assets` on Windows, `$XDG_DATA_HOME/axcenity/assets` (else `~/.local/share/axcenity/assets`) on Linux, `~/Library/Application Support/Axcenity/Assets` on macOS. `AXCENITY_ASSET_DIR` or an editor setting overrides it. Not the editor's roaming data folder, since it holds gigabytes |
| 5 | **A persistent per-asset metadata database** for source and licence records | `metadata.db`, an SQLite database in the asset directory, one row per asset (`provider:id`) that is never dropped when its files are removed, plus its downloads, the project files installed from it, favourites, collections and recents. It replaces the Blender pipeline's `registry.json` and the proposed sidecar `source` blocks. Projects keep a generated snapshot of their own assets' records (`.axcenity/asset-sources.json`) so credits survive moving a project to another machine; the database stays the source of truth |
| 6 | First sound source | Not answered yet; Openverse is still the recommendation. Needed only at migration step 7 |

The details are in [provider-interface.md](provider-interface.md), sections 3 to 5.

## The decisions as they were put

1. **Architecture.** The Toolbox as a native module embedded in the editor, with the Django site linked by its manifest file (recommended), rather than the Django app as a local companion service, a hosted backend, or a live link to it. See [architecture-options.md](architecture-options.md).
2. **Rust as the one implementation.** The Blender pipeline moves its provider calls onto the `axcenity-toolbox` CLI, source by source, and its Python providers are retired once each passes the same fixtures (recommended). The alternative keeps two implementations held together only by the shared record and fixtures.
3. **Porting your code into the Apache-2.0 engine.** Neither the Toolbox nor the 3D Models repository has a licence file. You wrote both, so you can relicense, but porting their logic into the engine should be your explicit choice. Recommended: yes, for the provider, record, variant, licence and download-safety logic.
4. **Where the shared library lives.** One per-machine folder that the editor and the Blender pipeline both point at (recommended: an editor setting, defaulting to the editor's data folder, with `PIPELINE_ASSET_LIBRARY` pointing the pipeline at the same place).
5. **Provenance in the project.** Each file the Toolbox installs gets an optional `source` block in its `*.import.json` sidecar (provider, id, licence, authors, page), so the editor can show licences and Build Game can write a `CREDITS.txt` (recommended; the alternative is one `.axcenity/sources.json`).
6. **The first sound source.** Openverse (no key, but 200 requests a day per machine, mixed licences, filtered to CC0 and CC BY by default) first, with Freesound's API using your own key later (recommended). Sounds are one of the four first asset types in the brief, so this is needed before the Toolbox's sound step.

The migration's first step (moving the HTTP and zip helpers out of the Get texture dialog into the new crate, with no behaviour change) is safe to start before these are answered, since every option needs it.
