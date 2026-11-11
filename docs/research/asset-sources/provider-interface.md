# Toolbox interface and migration plan

The interface the editor uses to reach the Toolbox, the one asset record, the provider trait behind it, the shared library on disk, the engine touch points, and the order of work that moves the engine's ambientCG code onto it and adds Poly Haven. Part of [Axcenity Toolbox in the engine](README.md); the reasoning for this shape is in [architecture-options.md](architecture-options.md), and the panel that uses it in [toolbox-experience.md](toolbox-experience.md). Dated 2026-11-11. Nothing here is built yet.

Rust note for readers new to it: a **trait** is an interface (a set of methods a type promises to have); `&dyn AssetSource` is a reference to "any type that implements the trait", chosen at run time. **serde** is the library that turns Rust structs into JSON and back.

## 0. The editor's interface to the Toolbox

The editor holds one `Toolbox` value and calls only these methods; it never names a provider. Each call blocks, so the editor runs it on a background thread and polls for the result, as Get texture does today.

```rust
pub struct Toolbox { /* sources, http, cache, library */ }

impl Toolbox {
    pub fn open(settings: &ToolboxSettings) -> Result<Toolbox, SourceError>;   // library folder, offline, enabled sources
    pub fn sources(&self) -> Vec<&SourceInfo>;                                  // names, kinds, credit lines, licences
    pub fn categories(&self, source: &str, kind: Kind) -> Result<Vec<Category>, SourceError>;
    pub fn search(&self, query: &Query) -> Vec<(String, Result<SearchPage, SourceError>)>; // one result per source
    pub fn record(&self, key: &str) -> Result<AssetRecord, SourceError>;        // "polyhaven:red_brick_03", with variants
    pub fn preview(&self, record: &AssetRecord) -> Result<PathBuf, SourceError>; // cached thumbnail in the library
    pub fn fetch(&self, key: &str, choice: &Choice, cancel: &AtomicBool,
                 progress: &dyn Fn(Progress)) -> Result<Download, SourceError>; // into the library, verified
    pub fn library(&self) -> &Library;   // downloads, favourites, collections, recents, history (section 3)
}
```

| Brief 3.2 asks for | Where |
|---|---|
| Search for assets | `search` (every enabled source, results per source, failures per source) |
| Retrieve metadata | `record` (the full record with variants) |
| Browse categories | `categories`, and `Query::categories` |
| Preview assets | `preview` (thumbnails); a sound's `preview_url` streams |
| Download assets | `fetch` (verified, cached in the library, never twice) |
| Import into projects | The editor's install step (section 4), using the record and the download |
| Manage collections | `library()` (favourites, collections, recents, history) |
| Provider errors | `SourceError`, one safe sentence each, per source |
| Cache and reuse | The on-disk cache (API responses) and the library (files), both shared with the CLI |

## 1. The asset record (`asset-source`, version 1)

One JSON shape for a provider asset, written by the Rust crate, read by the editor, the CLI's callers and the Toolbox's manifest. It is the Blender pipeline's `AssetRecord` (`tools/blender/providers/base.py`) with the Toolbox's variants and attribution added, so the pipeline's existing registry entries already read as version 1 (its `from_dict` ignores fields it doesn't know, and every new field is optional). The values below are illustrative.

```json
{
  "provider": "polyhaven",
  "id": "red_brick_03",
  "kind": "texture",
  "name": "Red Brick 03",
  "url": "https://polyhaven.com/a/red_brick_03",
  "description": "",
  "tags": ["brick", "wall"],
  "categories": ["brick"],
  "authors": ["Rob Tuytel"],
  "license": "CC0",
  "license_version": "1.0",
  "license_url": "https://creativecommons.org/publicdomain/zero/1.0/",
  "attribution": "\"Red Brick 03\" by Rob Tuytel, from Poly Haven, CC0 1.0",
  "resolutions": ["1k", "2k", "4k", "8k"],
  "maps": ["base_color", "normal", "roughness", "ao", "height"],
  "physical_size": [2.0, 2.0],
  "polycount": 0,
  "popularity": 18234,
  "method": "scan",
  "thumbnail_url": "https://cdn.polyhaven.com/asset_img/thumbs/red_brick_03.png?width=256&height=256",
  "preview_url": "",
  "attributes": {},
  "variants": [
    {
      "label": "2K JPG",
      "resolution": "2k",
      "format": "jpg",
      "package": "maps",
      "files": [
        {"url": "https://dl.polyhaven.org/file/ph-assets/Textures/jpg/2k/red_brick_03/red_brick_03_diff_2k.jpg",
         "path": "red_brick_03_diff_2k.jpg", "size": 1500000, "md5": "…", "map": "base_color"}
      ]
    }
  ],
  "extra": {}
}
```

| Field | Rule |
|---|---|
| `provider`, `id` | The key is `provider:id` (`polyhaven:red_brick_03`), as the Blender pipeline and its `pipeline-asset` `sources` already use. Provider keys are lowercase: `ambientcg`, `polyhaven`, `kenney`, `sketchfab`, `openverse`, `freesound`, `local`, `catalog` |
| `kind` | `texture`, `hdri`, `model`, `sound`, `sprites`. The editor shows `hdri` as a sky |
| `license` | A normalised code: `CC0`, `PD`, `CC-BY`, `CC-BY-SA`, `CC-BY-ND`, `CC-BY-NC`, `CC-BY-NC-SA`, `CC-BY-NC-ND`, `SAMPLING+`, `SKETCHFAB-STANDARD`, `SKETCHFAB-FREE-STANDARD`, `SKETCHFAB-EDITORIAL`, or `unknown`. The Blender pipeline's codes, plus Sampling+ from the Toolbox. `license_version` and `license_url` keep what the provider said; the Toolbox's display strings ("CC BY 4.0") are code + version |
| `attribution` | The provider's own credit text when it gives one (Openverse does), else empty; credit lines are made from the fields when needed |
| `maps` | Normalised map names: `base_color`, `normal`, `normal_dx`, `roughness`, `metallic`, `ao`, `height`, `opacity`, `emission`, `arm`. Informational; the engine still decides each file's role by name when it installs (`assets::texture_roles`) |
| `physical_size` | Metres, `[width, height]` or `[width, height, depth]`, or absent |
| `variants` | Empty in search results; filled by a full lookup. `package` is `maps` (loose files), `archive` (a zip to extract), `bundle` (a scene file and the files it includes, kept at their relative `path`), or `single`. `md5` when the provider publishes one. `map` per file when known |
| `extra` | Provider-private data needed to download. Nothing outside that provider may read it |

Rules, as the Blender pipeline's `docs/asset-metadata.md` already sets for its own records: readers ignore unknown fields; add a field here before writing it; bump the version only when a field changes meaning. A set of JSON fixtures in `crates/toolbox/tests/fixtures/records/` is the shared truth; the Blender pipeline and the Toolbox copy them into their tests.

## 2. The crate: `toolbox`

A new workspace crate, `crates/toolbox` (package `axcenity-toolbox`), editor-only like `cook`. No engine-crate dependencies, so its CLI builds quickly and nothing in the runtime can reach it by accident.

```
crates/toolbox/
  src/lib.rs          AssetSource, SourceInfo, Query, SearchPage, sources()
  src/record.rs       AssetRecord, Variant, RemoteFile, Kind, map names
  src/error.rs        SourceError (user-safe messages)
  src/http.rs         Http trait, the ureq client, the fixture client for tests
  src/cache.rs        on-disk JSON cache with time-to-live and offline fallback
  src/guard.rs        untrusted-data rules: https only, refused types, zip and glTF checks
  src/download.rs     verified downloads (.part, size, MD5), zip extraction, progress, cancel
  src/library.rs      the shared library: layout, registry.json, per-asset JSON, lock
  src/variants.rs     describing variants and choosing one (resolution, format, maps)
  src/license.rs      normalising licences, policy verdicts, credit lines and files
  src/ambientcg.rs    ambientCG, v3 API
  src/polyhaven.rs    Poly Haven
  src/bin/axcenity-toolbox.rs   the CLI
```

Dependencies: `ureq`, `zip`, `serde`, `serde_json` (all already in the workspace), and one new one, **`md-5`** (RustCrypto, MIT OR Apache-2.0) to check Poly Haven's published MD5s, pinned in the workspace with `cargo hakari generate` afterwards. No async runtime: calls block, and the editor runs them on its own threads as it does today.

### The trait

```rust
/// One asset catalog: ambientCG, Poly Haven, ...
pub trait AssetSource: Send + Sync {
    /// Who it is and what it has; constant.
    fn info(&self) -> &SourceInfo;

    /// One page of results. Records carry no variants yet.
    fn search(&self, cx: &Context, query: &Query) -> Result<SearchPage, SourceError>;

    /// The full record for one id, with its variants.
    fn record(&self, cx: &Context, id: &str, kind: Option<Kind>) -> Result<AssetRecord, SourceError>;

    /// The address to fetch a file from. Most sources return `file.url` as it is;
    /// Kenney and Sketchfab trade it for a short-lived signed link here.
    fn resolve(&self, _cx: &Context, file: &RemoteFile) -> Result<String, SourceError> {
        Ok(file.url.clone())
    }
}

pub struct SourceInfo {
    pub key: &'static str,          // "polyhaven"
    pub name: &'static str,         // "Poly Haven"
    pub home: &'static str,         // "https://polyhaven.com"
    pub credit: &'static str,       // "Powered by Poly Haven", shown wherever its results are
    pub kinds: &'static [Kind],
    pub licenses: Licenses,         // Licenses::All("CC0") or Licenses::PerAsset
    pub download_hosts: &'static [&'static str], // files may only come from these hosts
    pub credential: Option<&'static str>,        // env var for a token, e.g. "SKETCHFAB_API_TOKEN"
    pub opt_in: bool,               // left out of "all sources" searches unless enabled
}

pub struct Context<'a> {
    pub http: &'a dyn Http,         // ureq in the editor and CLI, recorded fixtures in tests
    pub cache: &'a Cache,
    pub offline: bool,
}

pub struct Query { pub text: String, pub kind: Kind, pub page: u32, pub per_page: u32, pub categories: Vec<String> }
pub struct SearchPage { pub records: Vec<AssetRecord>, pub next_page: Option<u32>, pub total: Option<u64> }

/// Every source this build knows, in shelf order.
pub fn sources() -> &'static [&'static dyn AssetSource];
```

Downloading is not per source: `download::fetch(cx, &library, source, &record, &variant, cancel, progress)` takes any variant's files, calls `resolve` for each, checks each address against `download_hosts` and `guard`, downloads to `.part`, verifies size and MD5, extracts an `archive` (images and models only, inside the folder, entry and size limits), checks a `bundle`'s glTF references stay beside it, and records the result in the library. A file already complete in the library is not fetched again.

`variants::choose(&record, &Preferences) -> Choice` is the Toolbox's `pick_bundle`: the wanted format, else the kind's fallback list; the wanted resolution, else the nearest below, else above; only the wanted maps from loose files; and a note for each difference ("No 8K here, so 4K, the nearest."). Get texture shows the notes instead of refusing.

`SourceError` is the Toolbox's rule in Rust: each variant's `Display` is a sentence safe to show (naming the provider and at most an HTTP status), and the details go to `log`. Variants: `Offline`, `Unavailable` (whole source down, or a missing token: try no more of its assets, the Blender pipeline's `ProviderUnavailable`), `RateLimited { retry_after }`, `NotFound`, `BadResponse`, `Refused` (a guard rule), `TooLarge`, `Io`, `Cancelled`.

Every request sends `User-Agent: AxcenityEngine/<version> (asset sources)`; the CLI sends the same with `cli`. Poly Haven's terms ask for a unique one; it must not imitate a browser.

### Each source

| Source | Search | Record and files | Notes |
|---|---|---|---|
| ambientCG | `GET /api/v3/assets?q=&type=&offset=&limit=&include=title,url,tags,thumbnails,maps,dimensions` (Toolbox `ambientcg.py`) | `?id=<id>&include=…,downloads`: one `archive` variant per zip ("2K-JPG") | An unknown id is 200 with no assets. Kinds: `material` → texture, `hdri`, `3d-model` → model. Search results cached 1 hour, records 12 hours |
| Poly Haven | `GET /assets?t=<type>` once per kind, cached 12 hours, searched locally by every word against name, tags and categories, ranked by tag hits then downloads (Blender `polyhaven.py`). Offline it still works | `/info/<id>` + `/files/<id>`: textures one `maps` variant per resolution and format (`nor_dx` left out when `nor_gl` exists, `arm` only when AO or roughness is missing); HDRIs one `single` per resolution (`.hdr`, `.exr`); models one `bundle` per resolution (`.gltf` plus `include`) | Show `credit` beside its results (ToS 2.5). Sizes in millimetres become metres. Never `.blend` |
| Kenney, Sketchfab, local folders, catalogues | Ported from the Blender pipeline when the CLI replaces its Python providers (step 7) | | Sketchfab is `opt_in`, needs `SKETCHFAB_API_TOKEN` from the environment for downloads, never stores or logs it, and drops the header on a redirect to another host |
| Sounds | Not chosen yet: Openverse (no key, 200 requests a day per client) or Freesound's API (a user's own key) | | The Blender pipeline's page-reading script is not ported |

## 3. The shared library on disk

The Blender pipeline's layout (`tools/blender/providers/asset_registry.py`), with `audio/` added:

```
<library>/
  textures/<provider>/<id>/<resolution>/   maps as downloaded
  hdri/<provider>/<id>/<resolution>/
  models/<provider>/<id>/<resolution>/     glTF and what it includes
  audio/<provider>/<id>/
  sprites/<provider>/<id>/<version>/
  previews/<provider>/<id>.png
  cache/<provider>/                        API responses (JSON), time-stamped
  metadata/registry.json                   {"version": 1, "assets": {"provider:id": entry}}
  metadata/<provider>/<id>.json            the same entry, per asset
```

An entry is `{record, downloads: {"2k": {resolution, folder, files, maps, bytes, source_urls, fetched}}, analysis, added, updated}`, paths relative to the library. Downloaded files are never changed. The Blender pipeline's materials folders stay its own.

- **Location.** An editor setting, **Asset library folder**, defaulting to the editor's data folder (`eframe::storage_dir("Axcenity Editor")/Asset Library`, beside Global Assets). The CLI takes `--library` or `AXCENITY_ASSET_LIBRARY`; the Blender pipeline points `PIPELINE_ASSET_LIBRARY` at the same folder. One library per machine, shared.
- **Two writers.** The editor and a Blender job can write at once, so every registry write takes `metadata/.lock` (created exclusively; one older than a minute is treated as left behind) and writes through a temporary file and a rename, as the Blender pipeline already does.
- **Compatibility test.** A small registry written by the Blender pipeline (a fixture with a few entries, not the real library) must read, and the crate's writes must read back in Python unchanged.

## 4. From the library into a project

The editor installs by copying, as Global Assets do, so projects stay self-contained and the player never reads the library.

| Kind | Lands in | Then |
|---|---|---|
| texture | `assets/textures/<Id>/` and `<Id>.texture.json` (as Get texture does now) | Same apply rules as today (`docs/editor/assets.md`) |
| hdri | `assets/sky/<id>_<res>.<hdr|exr>` | Ask "Use *name* as the sky?" (`app/sky.rs`) |
| model | `assets/models/<id>/` with the glTF's files at their relative paths | Through the Import dialog's checks (`docs/editor/import.md`) |
| sound | `assets/audio/<id>.<ext>` | |

Each installed file's `*.import.json` sidecar gets an optional `source` block (decision 5):

```json
{"format_version": 1, "uid": "ax://…",
 "source": {"provider": "polyhaven", "id": "red_brick_03", "license": "CC0", "license_url": "https://creativecommons.org/publicdomain/zero/1.0/",
            "authors": ["Rob Tuytel"], "url": "https://polyhaven.com/a/red_brick_03", "attribution": ""}}
```

Old readers ignore it, so `format_version` stays 1. From it the Assets panel's tooltip can name the source and licence, and Build Game writes `CREDITS.txt` (required credits first, optional thanks for CC0) using `license.rs`.

## 5. The CLI: `axcenity-toolbox`

The same commands as the Blender pipeline's `tools/library.py`, so its callers change little:

```
axcenity-toolbox search "red brick" --kind texture [--provider polyhaven] [--limit 15] --json
axcenity-toolbox info polyhaven:red_brick_03 --json
axcenity-toolbox fetch polyhaven:red_brick_03 --res 2k [--format jpg] --json
axcenity-toolbox list [--kind texture] [--query brick] [--downloaded] --json
axcenity-toolbox credits [--keys a:b,c:d] [--out CREDITS.md]
axcenity-toolbox stats --json
  common: --library <dir>  --offline
```

`--json` prints records and download entries in exactly the library's shapes. Exit codes: 0 done, 2 a `SourceError` (its safe message on stderr), 3 a source unavailable. In the Blender pipeline, one small Python class per source registers with the existing `register_provider` and runs the CLI, so `providers.search`, `providers.fetch` and the acquisition code don't change; `asset_registry.AssetLibrary` keeps reading `registry.json` directly. The binary is found from `AXCENITY_ASSETS` or `PATH`.

## 6. Importing a Toolbox pack

**File > Import > Toolbox pack…** reads an `axcenity-pack` version 1 manifest (`<slug>.axcenity-pack.json`).

- Refuse another `format` or a newer `version`, with a sentence saying so.
- List the items, their licences, the use checks that fail (for example a CC-BY-NC sound blocks commercial use), the total size, and where each lands, and download only on **Import**.
- ambientCG and Poly Haven items are fetched by `provider:id` through their source (verified, into the library), using the manifest's chosen variant label when it still exists, else `variants::choose`. Other items are downloaded from their listed URLs through the same guard (https, refused types, size limit), since no source exists for them yet.
- `import_path` is read under `assets/`, through `safe_path`; the Toolbox's blank-path defaults map to the engine's folders (`textures/` → `assets/textures/`, `hdris/` → `assets/sky/`, `models/` → `assets/models/`, `sounds/` → `assets/audio/`).
- The manifest's `credits` and each item's licence go into the sidecars, as in section 4.

Optional Toolbox follow-up, its own decision later: a manifest version 2 that adds each file's `md5` and `map` and the normalised licence code.

## 7. Engine touch points

| Where | Change |
|---|---|
| `Cargo.toml` (workspace) | Add `crates/toolbox` to members; pin `md-5`; run `cargo hakari generate` |
| `crates/toolbox/` | New, as section 2 |
| `crates/editor/src/ambient_cg.rs` | Steps 1 to 3: the dialog only, calling the crate (HTTP, parsing, extraction and `replace_dir` move out). Step 4: replaced by `toolbox_panel.rs` and a `toolbox_panel/` folder (cards, drawer, library views) with its actions in `app/toolbox.rs` |
| `crates/editor/src/asset_browser.rs` | The Get texture button and state (`use crate::ambient_cg::GetTexture`, the `get_texture` field, the button, the finished-download refresh) become a button that shows the Toolbox tab |
| `crates/editor/src/app/assets.rs` | Drops from a Toolbox card: download, install, then the existing drop code for that kind |
| `crates/editor/src/material_editor.rs` | **Make material** opens the new file; a map's menu gains **From Toolbox…** |
| `crates/editor/src/terrain_studio/layers.rs` | **Use texture set** gains **From Toolbox…**; a card dropped on a layer fills it |
| `crates/editor/src/app/search.rs` | A `SearchProvider` for library and favourite assets |
| `crates/editor/src/app/chrome/panes.rs` | The Toolbox dock tab beside Assets and Output |
| `crates/editor/Cargo.toml` | Depends on `toolbox`; drops `ureq` (only Get texture uses it) |
| `crates/assets/src/model/maps.rs` | Add `nordx` (DirectX normal) and `norgl` to `WORDS`, with a test, before Poly Haven textures arrive (see the assessment, section 3) |
| `crates/assets/src/texture_set.rs` | Unchanged format; `source` and `link` filled from the record |
| `crates/assets/src/asset_id.rs` | Read and write the optional `source` block in sidecars |
| `crates/editor/src/global_assets.rs` | The copy-into-project pattern to reuse for installs |
| `crates/editor/src/app/sky.rs`, `app/files/import.rs` | Install paths for skies and models |
| `crates/editor/src/build_game.rs` | `CREDITS.txt` from sidecar `source` blocks |
| `crates/editor/src/app/settings.rs` | **Asset library folder** setting |
| Player, server, `sim` | No change, and no dependency on `toolbox`; a check in `scripts/check-platform.*` that `cargo tree -p axcenity-player` doesn't contain it |
| Docs | `docs/architecture/crates.md` (new row), `docs/editor/assets.md` (the Toolbox panel, the library, pack import), `docs/project/licensing.md` (third-party asset credits, `md-5`), `docs/README.md` (a row once built) |

## 8. Migration plan

Each step is one pull request, leaves the editor working, adds its own tests, and is verified locally (`cargo nextest run -p axcenity-toolbox -p axcenity-editor`, clippy, fmt). No test reaches the network: sources run against recorded responses through the fixture `Http`. Each source also gets one `#[ignore]` live smoke test, run by hand.

| Step | What | Done when |
|---|---|---|
| 0 | Paul signs off the decisions in the [README](README.md) | Answers recorded in this folder |
| 1 | **Crate and helpers, no behaviour change.** Create `toolbox` with `Http`, `SourceError`, `guard`, zip extraction and the limited copy, moved out of `ambient_cg.rs`; the dialog calls them | Get texture behaves as before; its two tests move to the crate and pass; new guard tests (refused types, entry limit) |
| 2 | **Record and ambientCG v3.** `AssetRecord`, the trait, `ambientcg.rs` on v3, `variants::choose`. The dialog searches and downloads through the trait and offers the nearest package with a note instead of refusing | Fixture tests for search, an unknown id, a malformed response, and the choice notes; the v2 parser is gone |
| 3 | **Library, cache, verified downloads.** The shared library and its lock, the on-disk cache with offline fallback, `.part` downloads with size checks, the Asset library folder setting. Get texture fetches into the library, then copies into the project | A second project gets the same material without a request; offline search shows cached results; the Blender registry fixture reads and round-trips |
| 4 | **Poly Haven, and the Toolbox panel.** `maps.rs` fix first, then `polyhaven.rs` with MD5 checks (`md-5`). The Toolbox tab replaces the Get texture dialog: search across sources, Textures and Skies tabs, source and category chips, sort, licence filter, cached previews, the details drawer, drag or double-click to apply, **Make material**, and Terrain Studio's **From Toolbox…** ([toolbox-experience.md](toolbox-experience.md)) | Fixture tests for catalogue search, `/files` to variants, `nor_dx` handling and an MD5 mismatch; drawing tests for the panel narrow and wide (`AXCENITY_SNAPSHOTS`); a sky lands in `assets/sky` and asks to be used; a texture dropped on a part fills it as one undo step |
| 5 | **Provenance, licences and the library views.** Sidecar `source` blocks, `license.rs` (codes, policy, credit lines), the Assets panel tooltip, Build Game's `CREDITS.txt`; favourites, collections, recents, history and This project in the panel | A built game with a CC-BY asset has its credit line; a CC0-only project's credits list optional thanks; favourites and collections survive a restart and a second project |
| 6 | **Poly Haven models.** `bundle` variants into `assets/models/<id>/`, the glTF reference check, through the Import dialog | A Poly Haven model places in the viewport with its textures; a glTF pointing outside its folder is refused |
| 7 | **Sounds.** The first sound source (decision 6; Openverse recommended), the Sounds tab, preview playback, install to `assets/audio`, drop to make a Sound | Fixture tests for search, licence filtering and the anonymous page limit; a dropped sound plays in Play |
| 8 | **CLI and the Blender pipeline.** `axcenity-toolbox`; in the 3D Models repository (its own pull request), CLI-backed `ambientcg` and `polyhaven` providers pass that repository's existing provider tests, then the Python versions are removed. Kenney, Sketchfab, local folders and catalogues port next, in the same way | The pipeline's `tools/library.py search/fetch` give the same records and files through the CLI |
| 9 | **Toolbox packs.** Import a Django Toolbox pack (section 6) and export a collection as one | The Django Toolbox's golden manifest imports, with its licence warnings shown; an exported collection imports back unchanged |
| Later | A live link to the Django Toolbox if it's ever hosted, manifest version 2, more sources each on its own merits | Each on its own decision |

Steps 1 to 3 are refactoring with one visible improvement (nearest package); 4 to 7 deliver the Toolbox and its four asset types; 8 and 9 make it the one provider layer across the engine, Blender and the Django site. Step 8 can start any time after step 3.

## 9. Risks

| Risk | Mitigation |
|---|---|
| A provider changes its API | Fixture tests show what was expected; the live smoke tests find the change; `SourceError::BadResponse` keeps the editor running and says which source failed |
| ambientCG's API is run by one person and isn't promised to be stable | Cache aggressively, page modestly, keep downloads in the library so a project never needs it twice |
| Two programs writing the library at once | The lock and atomic writes (section 3) |
| Large downloads (an 8K EXR texture is hundreds of MB) | Show sizes before downloading (Poly Haven and ambientCG both give them), a per-file and per-asset limit (today's 2 GB), cancel at any point, `.part` files never count as downloaded |
| Malicious or broken files | `guard.rs`: https only, hosts from `download_hosts`, refused types (`.blend`, scripts, executables), zip entry and size limits, glTF references inside their folder, `.glb` magic bytes |
| Credentials leaking | Tokens only from the environment, never written to the library, cache, logs, sidecars or URLs; tests cover a redirect to another host |
