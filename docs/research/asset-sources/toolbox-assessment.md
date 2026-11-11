# Assessment: the Toolbox, the Blender pipeline's providers, and the engine's Get texture

What each code base does with asset providers, how sound it is, and what the engine should take from it. Part of [Axcenity Toolbox in the engine](README.md). Dated 2026-11-11; the commits read are in the README.

Evidence markers: **[read]** means read in the source; **[doc]** means taken from that project's own docs; **[web]** means checked on the provider's site on 2026-11-11.

## 1. The Axcenity Toolbox

### What it is

A Django 6 site, CIDM 3312 Project 2, styled like the editor, for finding free game assets on ambientCG, Poly Haven and Openverse, saving them to a per-member library, building packs, and exporting a pack as a JSON manifest "the engine can import" [doc: `CLAUDE.md`, `prd/axcenity-toolbox.md`]. It began as a console script (`asset_search.py` + `providers/`), which must keep working, and the site was built in 13 planned phases.

| Area | Facts |
|---|---|
| Stack | Python 3.13, Django 6.1, Django REST Framework, django-htmx, Tailwind and DaisyUI through a standalone binary (no Node), `requests`, SQLite, local-memory cache; `uv` only [read: `pyproject.toml`, `config/settings.py`] |
| Size | 289 tracked files. Apps: `providers` (plain Python), `catalog`, `packs`, `accounts`, `config` |
| Tests | About 880 test functions in 46 files, no test reaches the network (an autouse guard), a golden file locks the manifest [read: `tests/`] |
| Docs | Layered like the engine's: an index per area, one topic file per subject, a PRD and a phased plan, a dated research file on 12 asset APIs [read: `docs/`] |
| Deployment | Local development only: `DEBUG = True`, a public development `SECRET_KEY`, `ALLOWED_HOSTS` is localhost, SQLite, per-process cache. The PRD puts production deployment out of scope [read: `config/settings.py`] |

### The parts worth porting

These are the provider-specific lessons the engine would otherwise relearn.

- **A provider-neutral record** (`providers/records.py`): `AssetRecord` (provider, external id, kind, title, author, thumbnail, licence and its URL, attribution, source page, tags, sound preview) with `Variant`s ("2K JPG") of `VariantFile`s (URL, file name, size). Every module fills the same shape, so nothing above it knows a response format.
- **ambientCG on the v3 API** (`providers/ambientcg.py`): `GET /api/v3/assets` with `include=`, `offset` paging, kinds mapped to `material`, `hdri`, `3d-model`, one variant per download zip. ambientCG answers an unknown id with 200 and an empty list, not 404, which it handles.
- **Poly Haven's paging** (`providers/polyhaven.py`): `/search` is semantic and never comes back empty, so a search with zero `keywordMatches` is no results, and later pages stop at the keyword matches. `/search` has no offset, so page N asks for N × 12 and keeps the last 12. `/info` is cached 12 hours (the API's own `Cache-Control`). `/files` becomes variants: loose maps grouped by resolution and format, each bundle (glTF, Blend) its own variant with its included files at their relative paths.
- **Safe errors** (`providers/http.py`): every failure is one `ProviderError` whose message can be shown to a user (it may name the provider and an HTTP status, never exception text, a body or a header); details go to a logger. 429 repeats `Retry-After` only when it is a plain number. A malformed item is skipped, a malformed response is one clear error.
- **Telling files apart** (`catalog/files.py`): resolution, format and package (loose maps, bundle, archive, audio) from a variant's label and files; each texture file's map type read from the part of its name between the asset id and the resolution, so `rusty_metal_02_rough_2k.jpg` is roughness even though the asset's name says "metal".
- **Choosing what to download** (`catalog/files.py`, `pick_bundle`): the member's format, else the next in a per-kind fallback list; the member's resolution, else the nearest below, else the nearest above; only the member's map types from loose maps; and a note for each place the choice had to differ. This is exactly the dialog behaviour the engine lacks: today Get texture refuses when the exact package is missing.
- **A licence engine** (`catalog/licenses.py`, `packs/credits.py`): `license_key()` reads a licence however a provider writes it ("CC0 1.0", "CC BY-NC-SA 3.0", "Public Domain Mark", "CC SAMPLING+ 1.0"); `license_info()` says what it allows and asks; use verdicts for commercial, personal and internal use; credit lines; and a formal attribution notice. Only needed once a non-CC0 source arrives (Openverse, Sketchfab), but it is the most careful code here.
- **The pack manifest** (`packs/manifest.py`, `tests/golden/pack_manifest.json`): `format: "axcenity-pack"`, `version: 1`, the pack's details, each item's chosen variant's files with URLs and sizes, licence, author, attribution, source page and `import_path`, and a `credits` list for everything not CC0.
- **The Toolbox never downloads, hosts or proxies a provider's file** [read: `docs/ARCHITECTURE.md`]. The browser fetches and zips. That keeps it inside the providers' terms and is the right rule for any hosted form of it.

### Weaknesses, as they bear on the engine

| Finding | Effect | Evidence |
|---|---|---|
| It's Python and Django; the engine is Rust | Nothing can be linked in. Reuse means porting or running it as a separate process | [read] |
| It's built for local development only | Running it as a shared or hosted service needs a production configuration, a real database and cache, secrets handling and hosting the PRD never planned | `config/settings.py` |
| No JSON search API | Discover is HTML and HTMX. The JSON API covers the library (`/api/library/`) and read-only packs (`/api/packs/`, `/export/`). A companion service would need new endpoints first | `docs/architecture/api.md` |
| Provider calls run inside the web request | A Poly Haven search makes one `/search` and up to 12 `/info` calls in a row, each with a 10 s timeout; uncached, a page can take seconds | `providers/polyhaven.py`, `providers/http.py` |
| Stored assets are never refreshed | `Asset.objects.from_record` keeps the first save forever; download URLs that move go stale | `catalog/models.py` |
| The manifest has no checksums or map roles | Poly Haven publishes an MD5 per file, which the engine could verify; the engine must re-derive each map's role from file names | `tests/golden/pack_manifest.json` |
| Default import paths don't match the engine's folders | Blank paths become `<kind>s/<id>/` (`textures/`, `hdris/`, `models/`, `sounds/`) at the project root; the engine keeps assets under `assets/` with skies in `assets/sky` and sounds in `assets/audio` | `docs/architecture/exports.md`, `docs/editor/assets.md` |
| The console API is frozen | `search()` / `get_details()` must not change (a course requirement); irrelevant to the engine, but the reason each module carries two APIs | `CLAUDE.md` |

### Secrets

- **No provider API keys exist in the Toolbox.** All three sources are used anonymously [read: grep for key, token, secret and environment variables across the source].
- `SECRET_KEY` is a published development value, not a secret worth keeping.
- **`db.sqlite3` holds members' accounts and their DRF API tokens.** It is not tracked by git. It must never be copied into the engine repository or any document. Nothing in this proposal reads it.
- `library_client.py` takes a token from `TOOLBOX_TOKEN` or a hidden prompt, never from a file.

### Area by area (brief 3.1)

| Area | What's there | Reuse, modify or redesign for the engine |
|---|---|---|
| Django architecture | Five apps: `config`, `providers` (plain Python, no Django), `catalog`, `packs`, `accounts`; thin class-based views over service modules (`catalog/library.py`, `packs/services.py`), each workflow in one transaction | Redesign: the engine needs a module, not a site. The clean split (providers know nothing of Django) is what makes porting easy |
| Applications | `catalog`: Discover, the library, saving, review status; `packs`: packs, gallery, export, downloads, attribution centre, demo data; `accounts`: sign-up, settings, download defaults, API token page | Reuse the ideas: library → the Toolbox's library views; packs → collections; download defaults → the panel's defaults. Accounts stay web-only |
| API integrations | ambientCG v3, Poly Haven, Openverse audio, each with the console API and the Toolbox API side by side; all through one `get_json()` | Reuse: port the Toolbox API halves to Rust providers |
| Discovery and search | Discover shelves per kind and provider, Load more paging, Openverse licence filters, provider status | Reuse the paging rules; replace Poly Haven `/search` + `/info` with the Blender pipeline's cached catalogue search (one request, works offline) |
| Metadata | `AssetRecord` → stored `Asset` (shared, first save wins) with variants as JSON | Reuse the record (merged into the one record); modify: records refresh on a time-to-live instead of never |
| Categorisation | Four kinds (`texture`, `hdri`, `model`, `sound`) and normalised tags; provider categories are not used | Modify: add provider categories for browsing |
| Retrieval and downloading | None on the server: the browser fetches and zips the files the server lists | Redesign: the engine downloads, verifies and extracts itself (the Blender pipeline's downloader and guard) |
| Authentication and keys | Django sessions, DRF token auth for the API, token rotation; no provider keys at all | Not needed in the engine now; sources that need keys read them from the environment |
| Caching | Django's per-process memory cache: searches 1 h, Poly Haven `/info` 12 h, drawer records 1 h, failures never cached | Modify: an on-disk cache shared with the CLI, usable offline, same lifetimes and the same never-cache-failures rule |
| Database models | `Asset`, `Tag`, `SavedAsset` (notes, rating, status), `StatusChange`, `Pack`, `PackItem` (variant and import path per pack), Django users and tokens; SQLite | Redesign: no database in the editor; the library registry and `toolbox.json` hold what one machine needs |
| Frontend | Server-rendered templates with HTMX, Tailwind and DaisyUI, styled after the editor, light and dark | Reuse the layout ideas (cards, drawer, filters, licence pages) in egui; the markup itself can't move |
| Error handling | `ProviderError` with safe messages, details logged, malformed items skipped, 429 handling | Reuse as `SourceError` |
| Tests | ~880, offline by rule, a golden manifest | Reuse the cases as Rust fixture tests; reuse the golden manifest for the pack importer |
| Limitations | Development-only settings, provider calls inside requests, no JSON search API, records never refreshed, no file checksums in the manifest | See the weaknesses table above |

### Verdict

Good, careful code with strong tests and docs, built as a website. Its value to the engine is its **knowledge** (paging quirks, safe errors, variant and licence handling, the library and pack ideas, the manifest), not its runtime. The Toolbox the engine talks to should be a native Rust module built from that knowledge; the Django site stays as an optional web companion, linked by the manifest.

## 2. The Blender pipeline's provider layer ("3D Models")

### What it is

Paul's Blender asset toolkit. `tools/blender/providers/` is plain Python (no `bpy`), used by Blender add-on code and by a CLI, `tools/library.py` (`search`, `fetch`, `list`, `info`, `refresh`, `credits`, `stats`, `masks`, `source`) [read].

| Part | What it does |
|---|---|
| `base.py` | The provider contract (`name`, `kinds`, optional `formats`, `licenses`, `opt_in`; `search`, `info`, `download`), `AssetRecord`, normalised map names (`base_color`, `normal`, `normal_dx`, `roughness`, `metallic`, `ao`, `height`, `opacity`, `emission`, `arm`) and a file-name map reader that knows `nor_gl` and `nor_dx`; resolution tokens ("2k" = 2048) and `pick_resolution` (largest not above, else smallest) |
| `asset_registry.py` | The shared library on disk: `textures/`, `models/`, `hdri/`, `sprites/` under `<provider>/<id>/<resolution>/`, previews, a material folder, `cache/<provider>/`, and `metadata/registry.json` plus one JSON per asset. Each entry holds the record, every downloaded resolution (files, maps, bytes, source URLs, time) and analysis results. Paths are relative to the library, so it can move. Offline search over the registry |
| `asset_downloader.py` | JSON GETs cached on disk with a TTL and reused when the network fails or `offline` is set; downloads to `.part`, checked against the provider's size and MD5, then renamed; already-complete files skipped; retries |
| `download_guard.py` | Downloaded data is untrusted: https only, refused extensions (`.blend`, scripts, executables), zip limits (4 GB unpacked, 5,000 entries), only image and model files extracted, a glTF may only reference files beside it, a `.glb` must start with `glTF` |
| `license_manager.py` | Licence codes (`CC0`, `PD`, `CC-BY`, and restricted `CC-BY-SA`, `-ND`, `-NC`…, Sketchfab's own), a normaliser, a per-config policy (default: CC0 and PD only), `verify` verdicts, `problems`, credit lines and a Markdown credits file; per-provider terms |
| Sources | `ambientcg.py` (v2 API, materials), `polyhaven.py` (`/assets?t=` catalogue fetched once and searched locally; textures one file per map, HDRIs one `.hdr`, models glTF plus includes), `kenney.py` (no API: pack pages and the site's own signed download link), `sketchfab.py` (public search, downloads with `SKETCHFAB_API_TOKEN`), `local_files.py` (your own folders, licence from a sidecar), `catalogue.py` (JSON lists of direct links) |
| `docs/asset-metadata.md` | The pipeline's per-pack asset record (`pipeline-asset` v1), which cites library sources by `provider:id` with licence, URL and authors |

Freesound is not a provider here: one pack script, `packs/abandoned_farmhouse/scripts/fetch_audio.py`, reads each sound's public page to check for CC0 and saves the CDN preview MP3 with a small JSON sidecar into `asset_library/audio/freesound/` [read].

### Strengths

- The most complete download side of the three: a per-machine library, verified and resumable downloads, offline reuse, and a real registry. The engine has none of this.
- The best untrusted-data rules of the three (`download_guard.py`). The engine's Get texture already rejects zip paths that escape, but not refused file types, entry counts or glTF references.
- Poly Haven's `/assets?t=<type>` catalogue searched locally: one cached request per kind instead of `/search` plus up to 12 `/info` calls, and it works offline.
- Kenney, local folders and catalogue files show the contract stretches past APIs.
- Sketchfab's token is read from the environment for each download and never stored, logged or put in a URL; headers are not repeated across redirects to another host.

### Weaknesses and risks

| Finding | Effect |
|---|---|
| ambientCG on the v2 API | Works today [doc: Toolbox research, live test 2026-10-30], but v3 is the documented current version |
| Licence note on Poly Haven is out of date | `PROVIDER_TERMS["polyhaven"]` says the API is free for non-commercial tools and a commercial product should contact Poly Haven. The current terms say it is free "for any purpose, including commercial use" and will "never require payment, a license, or a key" [web: Poly Haven API ToS 2.1, 2.3; API page dated 18 July 2026] |
| Freesound by page reading | `fetch_audio.py` sends a browser-like User-Agent and checks the HTML for a CC0 link instead of using Freesound's API. Fragile, and not the sanctioned route; the API needs a user's key, or Openverse returns the same sounds' licence fields without one |
| Kenney by page reading | No API exists, so this is the only way, but it breaks when the site's markup changes |
| The library sits inside the repository folder | `library.directory` is `asset_library` in the 3D Models project (git-ignored, with `PIPELINE_ASSET_LIBRARY` to move it). Sharing it with the editor needs one agreed per-machine place |
| Two records | `providers/base.py` `AssetRecord` (library sources) and `pipeline-asset` (pack assets) are different things; only the first is the provider record this proposal unifies. `pipeline-asset` stays the pipeline's own and keeps citing sources by `provider:id` |

### Secrets

No keys are stored. The only credential is `SKETCHFAB_API_TOKEN`, read from the environment. `asset_library/` (downloads, cache, previews) is not tracked by git.

## 3. The engine today

- **Get texture** (`crates/editor/src/ambient_cg.rs`, 916 lines; button and state in `crates/editor/src/asset_browser.rs`). Searches `https://ambientcg.com/api/v2/full_json` for `type=Material`, 24 a page sorted by popularity, shows cards with previews fetched six at a time, and downloads one exact package (1K to 8K, JPG or PNG; no substitute when it's missing) into `assets/textures/<AssetId>/` with a `<AssetId>.texture.json` (`axcenity_assets::texture_set_from_files`). The zip is written to a temporary folder, only images are extracted, paths that escape are dropped, a 2 GB limit applies, and the folder is swapped in at the end so a cancel leaves the project unchanged [read].
- **What's missing.** No Poly Haven, no HDRIs, models or sounds from any catalog, no shared download library (each project downloads again), no offline reuse, no record of licence or author beyond the texture set's `source`, `assetId` and `link`, and UI, HTTP, parsing and file work all in one file. Errors show `ureq`'s own text to the user.
- **Dependencies.** `ureq` 3 (rustls) and `zip` 2 are workspace dependencies; `ureq` is used only by Get texture, `zip` also by `project` (packages, backups) and `build_game.rs`. Neither is in the player [read: `Cargo.toml`].
- **A naming bug Poly Haven would hit.** `assets/src/model/maps.rs` joins up to three words of a file name and looks them up, but has `nrmdx` and `normaldx`, not `nordx`. Poly Haven's `name_nor_dx_2k.jpg` therefore reads as an OpenGL normal (its last word `dx` matches nothing, then `nor` does), and in a texture set holding both, `nor_dx` sorts before `nor_gl` and wins. Adding `nordx` and `norgl` with a test fixes it; the migration's Poly Haven step does that first.
- **Pieces to build on.** `global_assets.rs` already copies an asset's files from a per-machine library into a project at the same paths; `*.import.json` sidecars give every asset file a stable `ax://` ID; Asset Check, Find References and Rename or Move… work on whatever lands in `assets/`; the Import dialog brings models in; `app/sky.rs` asks before using an image as the sky.

## 4. What to reuse, by piece

| Piece | Best source | Engine destination |
|---|---|---|
| Asset record | Blender `AssetRecord` + Toolbox `AssetRecord` (variants, attribution) | `toolbox::AssetRecord` ([provider-interface.md](provider-interface.md)) |
| Provider contract | Blender `base.py` (kinds, formats, licences, opt-in) | `AssetSource` trait |
| ambientCG | Toolbox `ambientcg.py` (v3) | `toolbox::ambientcg`, replacing the v2 code in the dialog |
| Poly Haven | Blender `polyhaven.py` (catalogue search, downloads) + Toolbox `polyhaven.py` (variants from `/files`) | `toolbox::polyhaven` |
| Kenney | Blender `kenney.py` | Later; sprites have no engine use yet |
| Sketchfab | Blender `sketchfab.py` | Later, opt-in, token from the environment only |
| Sounds | Neither is right yet; Openverse (Toolbox) for no-key use, Freesound API with a user's key | Later, once a sound source is chosen |
| HTTP, cache, verified downloads | Blender `asset_downloader.py` | `toolbox::http`, `::download` |
| Untrusted-data rules | Blender `download_guard.py` + the engine's existing zip checks | `toolbox::guard` |
| Variant description and choice | Toolbox `catalog/files.py` | `toolbox::variants` |
| Licences and credits | Blender codes and policy + Toolbox `license_key()` reader and notice text | `toolbox::license` |
| Library on disk | Blender `asset_registry.py` layout and `registry.json` | `toolbox::library` |
| Pack manifest | Toolbox `axcenity-pack` v1 | An importer in the editor |
| Safe user-facing errors | Toolbox `ProviderError` | `toolbox::SourceError` |
