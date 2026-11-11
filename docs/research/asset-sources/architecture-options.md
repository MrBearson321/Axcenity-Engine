# Architecture options: how asset sources reach the engine

Four ways to connect the engine with the asset providers and with the Axcenity Toolbox, judged against the [development policy](../../development-policy.md) and what the code bases do today ([toolbox-assessment.md](toolbox-assessment.md)). Part of [Asset sources](README.md). Dated 2026-11-11.

## What the answer has to satisfy

- **Desktop first.** Windows and Linux are the primary targets. The editor must work offline with what it already has.
- **No infrastructure before it's needed** (policy rule 1): no hosted services, no extra runtimes to install, nothing that has to be kept running.
- **The player and server never depend on it.** Fetching assets is an editor job; a built game only reads files in the project (`docs/architecture/portability.md`, and as `ureq` and `zip` are kept out of the player today).
- **Providers' terms.** A unique User-Agent and a visible "where this comes from" for Poly Haven (ToS 2.4, 2.5); gentle request volume for ambientCG (one person runs it); Openverse's anonymous caps (20 a minute, 200 a day) are per client and must not be worked around.
- **One provider layer and one asset record** across the engine, the Blender pipeline and, as far as sensible, the Toolbox.
- **No secrets in any repository.** Today the only credential anywhere is an optional `SKETCHFAB_API_TOKEN` in the Blender pipeline's environment.

## The options

### A. Embedded

The provider layer is Rust, compiled into the editor (in its own crate). The editor calls the providers directly, downloads into a per-machine library, and copies into the project.

- **For.** One binary, nothing else to install or run. Works offline from the library and cache. Full control of threading, cancellation and progress inside the editor. Fits the existing Get texture code, which is already this shape. Testable with `cargo nextest` and recorded fixtures, no network.
- **Against.** The Toolbox's and the Blender pipeline's Python logic has to be ported (a few thousand lines between them for the parts worth having). Each provider API change needs an editor release to fix (as today).
- **Cost.** Medium: a new crate and a rewrite of the Get texture dialog, spread over several small PRs.

### B. Companion service (the Toolbox, run locally)

The editor starts or connects to the Toolbox on `localhost` and asks it for searches and records over HTTP.

- **For.** Reuses the Toolbox's Python as it is; the browser UI is there for people who prefer it.
- **Against.** Every editor user needs Python 3.13, `uv` and the Toolbox's dependencies, or a bundled Python runtime, on every OS. The editor has to start, health-check and stop a server process and cope with a port in use. The Toolbox has no JSON search or record API today (Discover is HTML), so new endpoints are needed anyway. It never downloads files, so the editor still needs its own download, extraction and safety code. It doesn't serve the Blender pipeline either, which runs its own providers. Two languages to debug for one feature.
- **Cost.** High, and it adds runtime infrastructure the policy says to avoid.

### C. Remote backend (a hosted Toolbox or asset API)

The Toolbox, or a new service, runs on the internet; the editor calls it.

- **For.** Provider fixes land without an editor release; one place to cache provider responses for every user; accounts, shared packs and a public gallery come for free.
- **Against.** Hosting, a production configuration (the Toolbox is development-only: `DEBUG`, a public secret key, SQLite, a per-process cache), uptime, backups, security reviews and cost, for a feature the editor can do alone. Every editor install then depends on that service being up, and offline use needs a local fallback anyway. A shared server's IP would carry everyone's Openverse requests against one anonymous cap. If it proxied files it would take on the providers' bandwidth and terms; if it doesn't (the Toolbox's rule), the editor still needs its own downloader. The policy defers hosted services until a concrete need exists.
- **Cost.** Highest, ongoing.

### D. Hybrid

Some mix of the above. Two variants are worth separating:

- **D1, embedded plus a live Toolbox link.** The editor embeds the providers (A) and can also sign in to a Toolbox with an API token to list your packs and import them. The Toolbox already has the pack API (`GET /api/packs/?mine=1`, `/api/packs/<id>/export/`) and token sign-in. But there is no Toolbox running anywhere except your PC, so this is built for a service that doesn't exist yet, and it needs token storage in the editor.
- **D2, embedded plus a file link.** The editor embeds the providers (A) and imports a Toolbox pack from its exported `.axcenity-pack.json` file. No network link to the Toolbox, nothing to store; the manifest format is the contract. A live link (D1) can be added later on the same importer, if the Toolbox is ever hosted.

## Side by side

| | A. Embedded | B. Companion | C. Remote | D1. Hybrid, live | D2. Hybrid, file |
|---|---|---|---|---|---|
| Works offline | Yes, from the library | Only searches it cached | No, without a local fallback | Yes for sources; no for the Toolbox | Yes |
| Extra runtime for users | None | Python + Toolbox | None, but a network dependency | None | None |
| New infrastructure | None | A local server per user | A hosted service | A hosted Toolbox to be useful | None |
| Serves the Blender pipeline | Yes, through the same crate's CLI | No | Only if it also downloads | Yes, as A | Yes, as A |
| Provider fixes need | An editor release | A Toolbox update | A server deploy | An editor release | An editor release |
| Engine work | Medium | Medium (process handling) + Toolbox endpoints | Low in the editor, high on the server | Medium + token handling | Medium + a small importer |
| Fits the development policy | Yes | No (rule 1) | No (deferred) | Partly: builds ahead of need | Yes |

## Recommendation

**D2: embedded providers, with the Toolbox linked by its manifest file.** In practice that is option A plus one importer:

1. A new editor-only crate, `asset-sources`, is the one provider layer. The editor embeds it; it also builds `axcenity-assets`, a command-line tool that prints JSON, so the Blender pipeline (Python, inside Blender or not) calls the same code instead of its own providers. That gives one implementation without a service: a process call per search or fetch is cheap, and the shared on-disk cache and library mean the second caller pays nothing. Details and the record are in [provider-interface.md](provider-interface.md).
2. The Toolbox stays an optional web companion for browsing, curating and sharing packs. It keeps its browse-only Python clients (a web request is the wrong place to spawn a CLI, and it never downloads files), but its manifest carries the shared record's fields, and the editor's **Import Toolbox pack…** reads the file.
3. A live Toolbox link (D1) and a hosted service (C) are deferred, with a revisit point: when the Toolbox is deployed somewhere other people use. The manifest importer is where D1 would plug in.

**Why not B or C.** Both add infrastructure the policy says to avoid, neither removes the need for an engine-side downloader with its safety checks, and neither serves the Blender pipeline. The Toolbox's value to the engine is its provider knowledge, which porting captures without its runtime.

**What D2 gives up.** Provider fixes ship with the editor, not instantly. Accepted: ambientCG and Poly Haven APIs change rarely, the editor already has this exposure through Get texture, and the CLI lets the Blender pipeline pick up a fix by updating one binary.
