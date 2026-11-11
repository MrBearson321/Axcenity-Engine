# Versioning, releases, updates and the Version Explorer: audit and proposal

Phases 1 and 2 of Paul's brief "Automated Versioning, Release Management & Update System" (2026-11-11). **Approved 2026-11-11:** Paul accepted every recommendation below except D1, where he chose option (b): **make the engine repository public** and publish releases from it. Phase 3 is under way.

- **Date:** 2026-11-11
- **Engine:** `main` at `19152d8`
- **GitHub:** `MrBearson321/Axcenity-Engine`, read through the REST API (releases, tags, workflow runs and their job timings)

## The files

| File | What it answers |
|---|---|
| [audit.md](audit.md) | Phase 1: the version today, release history, build and packaging, GitHub configuration and cost, install locations, user data, every existing format version |
| [architecture.md](architecture.md) | Phase 2: versioning, channels, release data and the Version Explorer model, the release lifecycle, the cost-first Actions workflow, the editor UI, the updater, migration, security, tests and the roadmap |
| [schemas.md](schemas.md) | The files developers write (`releases/`), the signed manifest the engine reads, and the history file |

## Summary

**What exists.** The packaging is solid: `release.yml` builds and packages all three OSes, `check-packages.py` verifies them locally, and one release has been published, `v0.1.0-alpha.3` (2026-10-21). The project format already has its own version (4) and a step-by-step migration with backups, and user data is already kept out of the install folder.

**What's missing or wrong.**

- The installed alpha.3 calls itself `0.1.0`: the tag and the code are two version sources that nothing compares.
- No build records its commit. There's no changelog, no machine-readable release data, no channel concept, and no way for the engine to learn about a new release.
- **The repository is private**, so the engine can't check for or download releases without a GitHub token, and every Actions minute is billed (macOS at about 10×).
- Projects are upgraded to a new format on open without asking first, and don't record which engine saved them.

**The proposal in one paragraph.** `Cargo.toml`'s version becomes the one version (with its `-alpha.N` suffix), and the channel follows from it. Each PR adds a small TOML change entry under `releases/`; a new `crates/release` turns those, plus a feature registry and a known-issues list, into release notes, a signed JSON manifest and a history file. The same crate answers the Version Explorer's queries for the editor and for the MCP toolkit. A tag starts one cost-tiered GitHub Actions run that validates on Linux before building anything, builds only Windows and Linux for alphas, and ends in a **draft** that Paul publishes. The editor checks GitHub at most daily, shows one quiet notification, downloads and verifies the update itself, then hands over to a small separate updater that swaps the install folder after the editor exits and rolls back on failure. Opening an older project asks before migrating and validates afterwards.

## Decisions for Paul

Each has a recommendation. Phase 3 can start once D3–D6 and D12 are settled; the others are needed by the phase named. **Outcome (2026-11-11):** all recommendations approved; D1 resolved as (b), the engine repository becomes public, so releases, update checks and downloads use it directly and the releases-only repository is not created.

| # | Decision | Recommendation | Alternatives | Needed by |
|---|---|---|---|---|
| **D1** | Where do users get releases from, given the engine repository is private? | **A public releases-only repository** (e.g. `MrBearson321/Axcenity-Releases`) holding GitHub Releases and nothing else. The private repository's workflow uploads drafts there with a fine-grained token limited to that repository. Source stays private; users need no token | (b) Make the engine repository public: free standard Actions minutes, Sigstore attestations become available, simplest setup; but publishes the source. (c) Stay private and require a GitHub token in Settings: only people with repo access can update. (d) Also run the *builds* in the public releases repository to get free minutes: cheapest, but its build logs would be public | Phase 4 |
| **D2** | GitHub Actions scope and cost | Tag-triggered `release.yml` only, plus a manual `dry_run` (~1 Linux minute). `ci.yml` changed to manual-only. Validate on Linux before any build. **Alphas build Windows and Linux only, `release` profile; betas add macOS arm64; stables add universal macOS with the `dist` profile.** Every run ends in a draft that Paul publishes. Paul is told before each tag is pushed | Keep macOS in every release (≈3–4× the cost of an alpha); keep `dist` for alphas (slower, more minutes) | Phase 4 |
| **D3** | The one version source | **`[workspace.package] version` in `Cargo.toml`, including the pre-release suffix**; the workflow fails if the tag differs. Replaces "the crate version stays `0.1.0`" in `releases.md` | Keep the tag as the source and inject it at build time (two sources again) | Phase 3 |
| **D4** | Channels | `-alpha.N` = **Development**, `-beta.N`/`-rc.N` = **Beta**, no suffix = **Stable**. Default channel = the installed build's. **No nightly builds** (cost) | A separate `-dev.N` suffix for Development and alphas in Beta | Phase 3 |
| **D5** | The first version under this system | **`0.2.0-alpha.1`** (format 2→4 and multiplayer since alpha.3 are a minor bump) | `0.1.0-alpha.4` | Phase 4 |
| **D6** | Where release notes come from | **One TOML change entry per PR in `releases/unreleased/`**, with categories, subsystem, features, issues and a `public` flag. Adds `toml` and `semver` as direct workspace dependencies (both families already in the lockfile) | Conventional commits, PR labels, or one `CHANGELOG.md` (conflicts between parallel PRs) | Phase 3 |
| **D7** | Signing | **Ed25519 (minisign) signature over the manifest**, public key compiled in; **secret key as an Actions secret** used only by the draft step. Adds `sha2`, `minisign-verify` and `tar`. Windows/Apple code signing stays deferred (paid certificates) | Paul keeps the key offline and signs each draft's manifest by hand: safer, one manual step per release | Phase 4 |
| **D8** | Which installs update themselves | **Windows setup, Linux `.tar.gz` and AppImage.** `.deb`/`.rpm`/pacman and macOS get a notification with the download and the command | Also macOS now (unsigned app: Gatekeeper friction) | Phase 6 |
| **D9** | Updater design | **Separate small `axcenity-updater` binary**; the editor downloads and verifies with its own progress UI; the new release's updater runs from a temporary copy, swaps the install folder after the editor exits, verifies, keeps one previous install for Roll back | Run the verified installer silently (less control over rollback); updater does the download too | Phase 6 |
| **D10** | Project migration | **Ask before migrating** (today it's automatic on open), describe each step, keep the backup, validate after. **Project format 5** records `created_with`/`saved_with` engine versions (needs a bump because the manifest rejects unknown fields) | Keep silent upgrades; store the engine version in a side file to avoid a format bump | Phase 7 |
| **D11** | Update checks by default | **On, at most once a day**, one quiet notification per new version, official builds only | Off until the user turns them on | Phase 5 |
| **D12** | Seeding history | alpha.1 and alpha.2 recorded as **tags never released**; alpha.3 as the **one verified release**; everything since in the first new release, each entry **marked reconstructed** with its PR. About 40 subsystem-level features to start, each with its evidence | Start the history empty at 0.2.0-alpha.1 | Phase 3 |

## For the MCP toolkit thread

The Version Explorer's data model is UI-free (`crates/release`: `History`, `ReleaseRecord`, `Feature`, `KnownIssue`, `Version`), and every question is a function returning a small serializable answer: `introduced_in`, `feature_timeline`, `compare`, `issue_status`, `issues_affecting`, `minimum_engine_for`, `search`. The same queries run from the command line with `--json`. The toolkit can link the crate or call the CLI; it never needs the whole history. Details: [architecture.md §3](architecture.md#the-data-model).
