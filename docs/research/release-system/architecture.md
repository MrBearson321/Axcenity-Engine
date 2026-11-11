# Phase 2: architecture of versioning, releases, updates and the Version Explorer

The proposal for brief sections 1–18, built on the facts in [audit.md](audit.md). The file shapes are in [schemas.md](schemas.md). Decisions that need Paul are marked **(D*n*)** and listed in the [README](README.md#decisions-for-paul).

## 1. Principles

1. **One fact, one place.** The version lives in `Cargo.toml`. What changed lives in `releases/`. What the build produced (commit, checksums, toolchain) is added by the release workflow. Every other copy (installer version, notes, manifest, the editor's history, what the MCP toolkit reads) is generated from those.
2. **Cost first on GitHub Actions** (Paul, 2026-11-11). Actions run only on a release tag or by hand, never on push or PR. A one-minute Linux job validates everything before any expensive build starts. Paul is told before a run that will publish.
3. **No infrastructure before it's needed** ([development policy](../../development-policy.md)). GitHub Releases are the distribution server; there is no update server, database or website.
4. **The install folder is the only thing an update touches.** Projects, settings, plugins, assets and saves already live elsewhere ([audit 6](audit.md#6-user-data-and-where-it-lives)).
5. **Every step is testable on a developer machine.** The workflow YAML only calls repository scripts and `axcenity-release` commands that run locally first.

## 2. Versioning

### The one version

The authoritative version is `[workspace.package] version` in the root `Cargo.toml`, including its pre-release suffix: `version = "0.2.0-alpha.1"` **(D3)**. Every crate already inherits it, so `env!("CARGO_PKG_VERSION")` is the product version everywhere (editor, player, server, updater, `axcenity-test`).

- The tag must equal it: the release workflow's first job fails if `v$(cargo version)` isn't the tag. Today the tag *defines* the installer's version and the code says `0.1.0`; this reverses that so they can't disagree.
- `docs/project/releases.md`'s rule "the crate version stays `0.1.0` until 0.1 is stable" is replaced.
- Engine crates aren't published to crates.io, so a pre-release suffix on them costs nothing.

### Choosing the number

SemVer 2.0, with pre-1.0 meaning as Cargo reads it:

| Change since last release | Bump while 0.x | Bump from 1.0 |
|---|---|---|
| Breaking: a script API removed, a format that older engines can't read, behaviour games rely on changed | minor (0.2 → 0.3) | major |
| New features | minor | minor |
| Fixes and small improvements only | patch | patch |

`axcenity-release prepare` suggests the bump from the change entries' categories (any `breaking` → minor while 0.x) and refuses a version that isn't greater than the last release.

The first release under this system is proposed as **`0.2.0-alpha.1`** **(D5)**: 458 commits since alpha.3 include project format 2 → 4 and multiplayer, which is a minor bump under the table above.

### Channels

The channel is a function of the version, never stored separately **(D4)**:

| Version | Channel | GitHub release |
|---|---|---|
| `0.2.0` | **Stable** | full release, marked Latest |
| `0.2.0-rc.1`, `0.2.0-beta.1` | **Beta** | pre-release |
| `0.2.0-alpha.1` | **Development** | pre-release |

- A user on Stable is offered only stable versions; Beta sees beta and stable; Development sees everything. Within those, the highest SemVer precedence wins (`semver` crate ordering: `0.2.0-alpha.1 < 0.2.0-beta.1 < 0.2.0-rc.1 < 0.2.0`).
- The default channel is the installed build's own channel, so someone who installed an alpha keeps getting alphas until they choose otherwise.
- **No scheduled nightly builds.** "Frequent development builds" are alphas tagged by hand when there is something worth testing, because each build costs Actions minutes.
- Three independent guards keep a development build out of Stable: the channel is computed from the version, the workflow refuses to publish a pre-release version as a full release (and the reverse), and the editor recomputes the channel from each manifest's version and drops any that disagree.

### Build identity and provenance

A small build script in the new `release` crate records, at compile time:

| Field | Local build | Official build (release workflow) |
|---|---|---|
| version | from `Cargo.toml` | same, checked against the tag |
| commit | `git rev-parse HEAD`, plus `-dirty` if the tree has changes; `unknown` without git | the tagged commit |
| tag | none | `v…` |
| build kind | `local` | `official` (the workflow sets `AXCENITY_OFFICIAL_BUILD=1`) |
| built at | `SOURCE_DATE_EPOCH` if set, else now | the tagged commit's time, so it doesn't vary between reruns |
| rustc, target triple, profile | from Cargo's build environment | same |

No new dependency: Cargo's own build-script environment and one `git` call. The build script reruns only when `HEAD` or the environment variables change, so it doesn't slow ordinary edit-compile loops. Only official builds check for updates; a `local` build shows "built from source at `<commit>`" instead.

`axcenity-editor --version`, `axcenity-player --version` and `axcenity-server --version` print this; `--version --json` prints it as JSON for scripts and agents.

## 3. Release history and the Version Explorer data model

### Sources and outputs

```
 written by people (git)           written by the build             generated
 ─────────────────────────         ─────────────────────            ───────────────────────────────
 Cargo.toml version   ───────┐
 releases/unreleased/*.toml ─┤                                 ┌──▶ GitHub release notes (Markdown)
 releases/history/*.toml  ───┼──▶ axcenity-release ◀── commit, ├──▶ axcenity-release.json (signed manifest)
 releases/features/*.toml ───┤     (one crate)        sha256s, ├──▶ axcenity-history.json (Version Explorer data)
 releases/issues.toml     ───┘                        toolchain├──▶ history compiled into the editor (offline)
                                                               ├──▶ docs page (later, optional)
                                                               └──▶ queries for the editor and the MCP toolkit
```

**(D6)** Release notes come from **change entries**: one small TOML file per change, added in the PR that makes the change ([schemas.md](schemas.md#a-change-entry-releasesunreleasedtoml)). The alternatives were considered:

- *Conventional commits*: history so far doesn't use them, a commit message isn't written for users, and squash/merge habits vary. Not required.
- *PR labels*: the repository has only GitHub's default labels, labels can't carry a user-facing sentence, and reading them needs the GitHub API at release time. Not used.
- *A single `CHANGELOG.md`*: every PR edits the same lines, so parallel threads (this project runs several) conflict constantly.

Change entries keep internal and experimental work out of user-facing notes by rule (`category = "internal"`, `public = false`, `stability = "experimental"`), satisfying "don't publish every commit as a feature".

### The data model

`axcenity-release` defines plain Rust types with no UI: `Version` (SemVer + channel), `ReleaseRecord`, `Change`, `Feature`, `FeatureEvent`, `KnownIssue`, `Formats`, `Evidence` (`Verified`, `Reconstructed`, `Unknown`). A `History` holds them all in memory; the whole public history of an engine this size is well under a megabyte, so no database is needed (the Toolbox's SQLite isn't reused for this).

Queries are functions returning small serializable answers, so the editor, a docs page and the MCP toolkit call the same code and an AI agent never needs the whole history:

| Query | Answers |
|---|---|
| `feature(id)` / `search_features(text, subsystem)` | a feature, its status and docs link |
| `introduced_in(feature)` | version + evidence + source ("verified: crates/terrain at v0.1.0-alpha.3") |
| `feature_timeline(feature)` | each version that changed it, with category and PRs |
| `compare(a, b)` | changes in `(a, b]` grouped by category, format changes, issues introduced and resolved, breaking changes |
| `issue(id)` / `search_issues(text, subsystem, status)` | an issue and its version history |
| `issue_status(id, version)` | fixed / confirmed affected / affected by range / not investigated ([rule](schemas.md#known-issues-releasesissuestoml)) |
| `issues_affecting(version)` | open and confirmed issues for a version, e.g. the installed one |
| `minimum_engine_for(format, n)` | the first release that reads project format *n* |
| `search(text, filters)` | all of the above, ranked, paged |

The command-line program exposes the same queries (`axcenity-release query compare 0.1.0-alpha.3 0.2.0-alpha.1 --json`). The MCP toolkit thread can wrap those calls or link the crate; the types are the contract.

### Seeding the history without inventing it (D12)

| Version | Recorded as | Evidence |
|---|---|---|
| `0.1.0-alpha.1` | tag only, never published | verified: tag `2e80234c9`, no GitHub release |
| `0.1.0-alpha.2` | tag only, release build failed | verified: tag `b59d2bf4d`, failed workflow run |
| `0.1.0-alpha.3` | published pre-release, 2026-10-21 | verified: GitHub release, tag `a897631ff`, run succeeded; formats read from the tree at the tag (project 2) |
| everything since | the first new release's record, entries marked `evidence = "reconstructed"` with their PR numbers | PR descriptions, roadmap report, `docs/benchmarks.md` |

- A feature is "introduced in 0.1.0-alpha.3 (verified)" only if the tree at that tag shows it (a crate, a module, a menu entry). Otherwise it's "introduced after 0.1.0-alpha.3 (reconstructed, PR #n)" or "unknown".
- The current state of `main` is never shown as a release. The editor labels it "unreleased (built from source)" for local builds.
- The feature registry starts small: about 40 features at subsystem level (Terrain Studio, Model Studio, multiplayer, blocks, …), not every function. More are added as change entries name them.

### Published releases are immutable

After a release is published, its record and manifest don't change. A mistake in the notes is fixed by a `[[correction]]` entry in the *next* release's record (shown on the old release in the Version Explorer, marked as a correction with its date). A broken release is fixed by a new patch or pre-release number, never by replacing files on an existing tag; the workflow refuses to upload to a release that is already published (today's `gh release upload --clobber` path is removed).

## 4. The release lifecycle

| Brief step | What happens | Where | Cost |
|---|---|---|---|
| 1–3. Develop, categorize, update feature/issue records | Each PR adds a `releases/unreleased/*.toml` entry and, if needed, a feature or issue row. `axcenity-release check` (also run by `check-platform`) validates them | local | none |
| 4. Prepare a candidate | `axcenity-release prepare 0.2.0-alpha.1`: bumps `Cargo.toml` and `Cargo.lock`, writes `releases/history/0.2.0-alpha.1.toml` from the entries and the code's format constants, empties `unreleased/`. A person writes the title and summary | local, a PR | none |
| 5. Validate | `axcenity-release check --release 0.2.0-alpha.1`; the local platform and package checks on Windows and Linux, recorded in `[verification]` | local, Paul's PC through the coordinator | none |
| — | **Tell Paul** the tag is about to be pushed, which version, which platforms, the estimated minutes | thread message | |
| 6–7. Tag, build, test | Paul pushes the tag (or says to). The workflow validates first, then builds, packages and smoke-tests | GitHub Actions | see §5 |
| 8. Publish notes and metadata | The workflow creates a **draft** release with packages, manifest, signature and history | Actions | under a minute |
| — | **Paul publishes the draft** (one click), the explicit go for users to see it | GitHub | none |
| 9. Version Explorer gets the history | The history file is attached to the release; the next editor build also compiles it in | automatic | none |
| 10–12. Editor detects, user updates, install verified | §6 and §7 | users' machines | none |

## 5. GitHub Actions: cost-first release workflow (D2)

### What runs when

| Workflow | Trigger | Runs |
|---|---|---|
| `release.yml` | push of a `v*` tag; manual dispatch with `dry_run` | the release pipeline below |
| `ci.yml` | **manual dispatch only** (today: every push and PR, currently disabled) | unchanged jobs, for when someone wants them |

Turning Actions back on for the repository doesn't re-enable workflows that were disabled by hand, but `ci.yml`'s triggers are changed anyway so a stray enable can't start per-push runs.

### The pipeline

```
validate (ubuntu, ~1 min)  ──▶  build (per platform)  ──▶  package + smoke test  ──▶  draft release
  tag == Cargo.toml version        cached deps                  update zip, installers      manifest, signature,
  releases/ records valid          profile by channel           --version --json check     history, notes
  channel ↔ prerelease flag
  tag not already released
```

- **Validate first, on Linux.** Every check that can fail cheaply runs before any Windows or macOS minute is spent. It's `axcenity-release check --tag $TAG`, the same command developers run.
- **Platforms and profile by channel** **(D2)**:

  | Channel | Builds | Profile | Rough cost (Linux-minute equivalents, scaled from alpha.3) |
  |---|---|---|---|
  | Development (alpha) | Windows, Linux | `release` (thin LTO) | ~100–150 |
  | Beta / RC | Windows, Linux, macOS arm64 | `dist` | ~300–450 |
  | Stable | Windows, Linux, macOS universal | `dist` | ~400–600 |

  macOS minutes cost about ten times Linux minutes, and macOS is a secondary platform, so alphas skip it; Mac users of an alpha build from source or wait for a beta. The fat-LTO `dist` profile is kept for releases meant for wider use.
- **Caching**: `Swatinem/rust-cache` keyed on OS, profile and `Cargo.lock`, saved only by tag runs (so dispatch dry runs don't evict it). Packaging tools (nfpm, appimagetool) are pinned and cached.
- **Tests in Actions are smoke tests only**: `axcenity-release` unit tests, `--version --json` of each built binary, Build Game for the host OS, and the player's first frame on Linux under `xvfb` (what `check-packages.py` already does). The full test suite and GPU tests stay local, per the development policy; hosted runners have no GPU, and running 3,000+ tests on three OSes per release would multiply cost.
- **Testing the workflow before it runs**: workflow steps call repository scripts, which are run locally first; `actionlint` checks the YAML locally; `workflow_dispatch` with `dry_run: true` runs only the validate job (about one Linux minute) to prove the trigger and permissions.
- **Draft, then Paul publishes.** The workflow never publishes. A failed job means no draft is created, so an incomplete build can't reach users. If the account's plan supports protected environments, the signing step also runs in a `release` environment that requires Paul's approval; if not, the draft is the gate.

### Release artifacts per platform

Existing packages stay as they are. One addition per platform: the **update payload**, the staged folder (`packaging/stage.py` output) as an archive, which is exactly what the installer would have put on disk. On Linux the existing `.tar.gz` already is that; on Windows a new `…-windows-x86_64-update.zip`. Plus `axcenity-release.json`, its `.minisig` signature and `axcenity-history.json`. `SHA256SUMS.txt` stays for people verifying by hand.

## 6. In the editor

### Where the version shows

| Place | What |
|---|---|
| About box (exists) | version, channel, build kind, commit, built at; **Copy diagnostics** button |
| Start page footer (exists) | version and channel, e.g. "Axcenity 0.2.0-alpha.1 · Development"; an "Update available" link when there is one |
| Status bar | nothing by default; a small update dot only when an update is available and not dismissed |
| Settings > Updates (new page) | channel, automatic checks on/off, check interval, last check result, Check now, Roll back to previous version |
| Help > Version Explorer (new) | the window below |

No version in the title bar or toolbar; the brief asks not to clutter.

### Version Explorer window

A dock tab built from the existing design system (`theme::palette`, `widgets.rs`, `responsive.rs`; fits the 260-point column rule):

- **Overview**: installed version and channel, latest available on the channel, Update button, the latest release's notes, known issues affecting the installed version.
- **Releases**: every release, newest first, with evidence badges (Verified / Reconstructed); click for full notes and GitHub links.
- **Features**: search and subsystem filter; a feature's timeline.
- **Issues**: search, status filter, "does this affect my version?" answered with the four-state rule.
- **Compare**: two version pickers; the `compare` query's output grouped by category, with format changes and breaking changes first.

It reads `History`: the compiled-in copy, replaced by a newer cached `axcenity-history.json` when one has been downloaded and verified. It works offline.

### Update checks (D11)

- **Official builds only.** Local and source builds never check.
- **Default**: automatic check at most once a day, a few seconds after start, in the background (the HTTP client is the existing `ureq`). Configurable interval (daily, weekly, never) and Check now.
- **One request per check**: `GET /repos/<owner>/<repo>/releases?per_page=20` (60 unauthenticated requests per hour is far above need), then the manifest and signature of the best candidate only.
- **Unobtrusive**: a new version produces one quiet notification (the existing toast style) and the status-bar dot. Dismissing hides it until a *newer* version appears. Never a modal, never during Play.
- **Offline / failure**: the result is "couldn't check: no connection" in Settings > Updates and nothing else; the last good result and the cached history stay usable.
- **Compatibility shown before updating**: the candidate's project format against the open project's ("Projects you save with 0.3.0 can't be opened by 0.2.x"), its `breaking` entries, and its known issues.
- **No forced updates.** A future security policy could mark a release `critical` in its manifest, which only changes the notification's wording and colour; it never blocks the editor.

### Diagnostics

**Copy diagnostics** (About box, and Help > Report a Problem) puts on the clipboard: version, channel, build kind, commit, tag, built at, rustc, target, profile, OS name and version, CPU, GPU adapter and driver (from wgpu's adapter info, which the renderer already has), the open project's format and the engine that last saved it, known issues affecting this version, and the path of the latest log. No project contents, no paths beyond the log's. The player and server print the same through `--version --json`.

### As built in Phase 5

The editor-facing topic is `docs/editor/shell.md` ("Versions and updates"). Differences from the plan above: the update dot is a chip with the version beside the Settings gear, since the editor has no status bar; the Version Explorer reads the compiled-in history only (a downloaded `axcenity-history.json` comes with the updater); its Overview and the start page don't offer an Update button until the updater exists (Phase 6), and Settings > Updates links the release page instead; Copy diagnostics leaves out the OS version, CPU, the open project's format and the log path for now, and Help > Report a Problem isn't there yet.

## 7. Updater and installation (D8, D9)

### Which installs update themselves

| How it was installed | In-app update | Otherwise |
|---|---|---|
| Windows setup (per user) | **Yes** | |
| Linux `.tar.gz` | **Yes** | |
| Linux AppImage | **Yes** (replace the one file); not yet, see Phase 6 | |
| Linux `.deb` / `.rpm` / pacman | No | Notification links the new package and shows the `apt`/`dnf`/`pacman` command |
| macOS `.dmg` | No, for now | Notification links the new `.dmg` (in-app later, once builds are signed) |
| Built from source | No | "Built from source at `<commit>`" |

The editor knows which case it is from an `install.json` that the packaging writes into the install folder (`{"kind": "windows-setup"}`, `"linux-tarball"`, `"appimage"`, `"deb"`, …). No file means a source build.

### Pieces

- **`crates/release`** (library): versions, channels, the data model and queries, manifest parsing, signature and hash checks, the GitHub client, safe archive extraction. Used by everything below.
- **The editor**: checks, shows notes, downloads with progress and Cancel, verifies, and stages. Downloading in the editor gives the progress bar and Cancel in Axcenity's own UI.
- **`axcenity-updater`** (new small binary, no egui, no engine crates; `release` crate + `zip`/`tar` only): installs a staged, verified update after the editor has exited, and rolls back on failure. It has its own version constant and `--version`.

### The update, step by step

1. **Download** the manifest and signature; verify the signature with the public key compiled into the editor; check version > installed, channel allowed, platform present, `min_update_protocol` ≤ this editor's.
2. **Download** the update payload to `<app data>/Updates/<version>/download.part` with progress; Cancel deletes it. Resumes with HTTP range requests after an interruption. Size and SHA-256 must match the manifest before the file is renamed into place.
3. **Extract** to `<app data>/Updates/<version>/staged/` with the safe extractor (§9). Check every file against the payload's file list.
4. **Ask** the user to save; the editor offers Save All / Discard / Cancel as it does for closing.
5. **Hand over**: copy the *new* release's `axcenity-updater` (verified, from the staged folder) to `<app data>/Updates/<version>/run/` and start it with an instruction file (current install folder, staged folder, editor process ID, version, `update_protocol`). Then the editor exits. Running the copied updater means no program ever replaces its own running file, and an updater fix ships in the release that needs it.
6. **Wait** until the editor process has exited (with a timeout and a clear message if it doesn't).
7. **Swap**: rename the install folder to `<install>.previous` (same volume, so it's a rename, not a copy), then move the staged folder to the install path. On Windows also update `DisplayVersion` and `EstimatedSize` under the existing `HKCU` uninstall key. An AppImage is one file renamed the same way.
8. **Verify**: run the new `axcenity-editor --version --json` and compare version and commit to the manifest.
9. **Relaunch** the editor with the project that was open. On the next start the editor shows "Updated to 0.2.0-alpha.2" with the notes.
10. **On any failure in 7–8**: remove the half-installed folder, rename `<install>.previous` back, relaunch the old editor and show what failed. The log is `<app data>/Updates/update.log`.

One previous install is kept; Settings > Updates > Roll back swaps it back with the same updater. The next successful update replaces it.

### What the updater never touches

Only the install folder and `<app data>/Updates/`. Projects, settings, plugins, Global Assets, saves and the Toolbox directory are outside both. The updater refuses an install path that is a filesystem root, the user's home, or any folder without the `install.json` marker, and it never follows symlinks out of the install folder. A test installs into a scratch folder surrounded by fake user data and checks that every byte of it is unchanged afterwards.

### As built in Phase 6

`crates/updater` (`docs/editor/shell.md`, "Versions and updates", for what people see; `docs/project/releases.md`, "Checking the updater"). Differences from the plan above:

- The download, safe extraction and staging live in `crates/updater` (`download.rs`, `extract.rs`, `stage.rs`) rather than `crates/release`, so the release crate stays light for the player and server; the editor uses them from a thread. The download is `Updates/<version>/<file>.part` beside the archive.
- The AppImage doesn't update itself yet: Settings > Updates tells it to download the new one. Only the Windows setup and the Linux tar.gz do.
- The updater copies the staged folder to `<install>.new` before the editor has to close, then swaps by renames (`.previous`, `.failed`, `.rollback` beside the install). It refuses a relative path, a filesystem root, the home folder, a link, a folder without `install.json` and an install kind that doesn't update itself, and it deletes a sibling only when it is an Axcenity install.
- The Windows setup's `Uninstall.exe` isn't in the update zip, so the updater carries it to the new folder; the uninstaller removes `<install>.previous` as well. The uninstall entry's `DisplayVersion` is updated only when the entry's `InstallLocation` is this folder; `EstimatedSize` isn't.
- Roll back swaps `.previous` and the install, keeping the newer one as `.previous`, and checks only that the version put back runs.
- The relaunched editor opens the project folder that was open, if any. It reads `Updates/result.json` on its next start and removes update folders up to its own version. A downloaded `axcenity-history.json` still isn't used.

## 8. Project compatibility and migration (D10)

The project format stays independent of the engine version: it changes only when the files' shape changes, and the existing step-by-step migration (`crates/project/src/migrate.rs`: one function per format step, a copy kept in `.axcenity/previous/<n>/`) is the right foundation. The changes:

1. **Record the engine.** Project format 5 adds `"engine": {"created_with": "0.2.0-alpha.1", "saved_with": "0.2.0-alpha.1"}` to the manifest. Because the manifest denies unknown fields, this has to be a format bump with its own (trivial) migration step; older engines then refuse a format-5 project with the message they already show, which is correct.
2. **Ask before migrating.** Opening an older project shows a dialog instead of upgrading silently:
   - what changes, one line per step (each step gets a description and the systems it affects: "Animation clips: converted to the one clip format"),
   - where the backup goes (`.axcenity/previous/<n>/`, or Back Up Project to a zip),
   - **Upgrade** / **Cancel**. Command-line tools (`--build-game`, `axcenity-test`) keep upgrading without asking but print the same report.
3. **Validate afterwards.** Reload the upgraded project and run the existing `project::validate`; report problems in the same dialog with the backup's location. If loading fails, put the backup back.
4. **Classify on open**, in this order: current → open; older and every step known → ask to migrate; older than the oldest step → "needs an older engine: this project was last saved with 0.1.0-alpha.3"; newer → "needs Axcenity *x* or later" (from `minimum_engine_for`), with a link to that release; unreadable manifest → "not an Axcenity project or damaged".
5. **Other formats** keep their own constants and handlers (saves, `.axgame`, network protocol, Toolbox database). Each release records all of them ([schemas.md](schemas.md#a-release-record-releaseshistoryversiontoml)), and `prepare` refuses a release where one changed without a change entry saying so. The Toolbox's database and asset record are read from the Toolbox crate's own constants, not redefined here.

"Opens but must migrate before editing" (brief 16.C.2) would need a read-only project mode the editor doesn't have. It isn't proposed now; Cancel plus the "needs an older engine" link covers the safe path.

The editor's own settings (eframe RON) have no version today. One is added only when a settings change first needs a migration (development policy rule 1).

### As built in Phase 7

User-facing and format details: `docs/project/layout.md` (the upgrade bullet). Differences from the plan above:

- `created_with` is left out of a project upgraded from format 4, since nobody recorded it; only new projects have it. A format 5 manifest without `engine` still loads.
- The classification is `project::compatibility` (no writes); "unreadable manifest" is `Damaged`, which the editor still opens as before (a broken manifest gives the folder's name), so only too old and too new are refused.
- The dialog lists the steps and where the copy goes; it doesn't offer Back Up Project to a zip, which needs an open project.
- "Validate afterwards" runs the Asset Check (`project::check_assets`) once after an upgrade and warns with the count. The files are put back only when the world read before the upgrade and doesn't after it, and only when this upgrade made the copy.
- An engine from now on refuses a manifest it can't parse whose `format_version` is newer. Engines from before this phase open a format 5 manifest as damaged (its `engine` field is unknown to them) with the folder's name; they don't write `format_version` back, so the project stays format 5.
- `prepare` doesn't yet fill a release's `formats` or check them against the change entries; that's left to Phase 8, with the first release.

## 9. Security and integrity (D7)

| Threat | Defence |
|---|---|
| Tampered download or manifest | **Ed25519 signature (minisign format)** over `axcenity-release.json`, verified with a public key compiled into the editor and updater (`minisign-verify`, pure Rust, no dependencies, MIT). Every other file is checked by size and SHA-256 (`sha2`) from that signed manifest |
| Download redirected to another host | Artifacts are file *names* in the signed manifest; URLs are built from the compiled-in release source. HTTPS only (rustls), no plain HTTP fallback; redirects only to GitHub's release asset hosts |
| Downgrade or channel swap | Version must be greater than installed unless the user chose Roll back; channel recomputed from the signed version |
| Incomplete download | `.part` file, size and hash checked before use; a failed check deletes it |
| Malicious archive (path traversal, zip bombs) | Extractor rejects absolute paths, `..`, drive prefixes, symlinks and device files; caps entry count, per-file and total size (the `.axgame` unpacker's limits are the model); extracts only into the staging folder |
| Downloaded code run unchecked | Nothing downloaded runs except the new updater and editor, and only after the signature and hashes pass. No scripts, no installers |
| Privilege escalation | No admin rights asked or needed: Windows installs are per user; system-wide Linux packages are left to the package manager |
| Leaked private information | Only `public = true` entries reach published files; a test checks the history file has no internal entries. Issue and PR numbers link to a private repository only if D1 keeps it private, in which case links are omitted from published data |
| Key compromise | Key rotation by shipping a new public key in a release signed by the old one; documented in the runbook |

**Signing key** **(D7)**: generated once by Paul. The recommended place for the secret key is a GitHub Actions secret, used only by the draft step (in a protected `release` environment if the plan allows). The alternative is Paul keeping it offline and signing each draft's manifest by hand, which is safer and costs nothing but is a manual step per release.

**OS code signing** (Windows Authenticode, Apple notarization) is separate from update integrity and costs money per year. It stays deferred; the minisign signature protects updates either way, and SmartScreen/Gatekeeper warnings remain for first installs.

## 10. Testing

TDD per phase; each list is what that phase's tests prove.

| Area | Tests (all local, no network unless marked) |
|---|---|
| Versions | parsing, precedence, channel mapping, `prepare`'s bump suggestion, rejection of non-increasing versions |
| Records | every schema rule in [schemas.md](schemas.md) as a failing fixture; internal entries never published; format change without an entry refused |
| Queries | introduced_in with each evidence level, compare ranges inclusive/exclusive, the four-state issue rule, search ranking and filters |
| GitHub client | a fake `Fetch` trait for unit tests; one test against a loopback HTTP server (std `TcpListener`, no new dependency) serving canned GitHub JSON: drafts ignored, pre-release flags, rate-limit and 404 handling, offline |
| Manifest | good, unsigned, wrong key, altered byte, wrong channel, unknown platform, lower version, unknown fields ignored |
| Download | interrupted and resumed, cancelled, size mismatch, hash mismatch |
| Extraction | traversal, absolute, symlink, bomb, too many entries |
| Updater | install into a scratch "install" with fake user data around it; failure injected at each step → rollback restores the old version byte for byte; user data unchanged; editor still running → waits; roll back command |
| Migration | the dialog's step list per format; validate after; failure restores backup; format 5 step |
| End to end | a scripted release from fixture records → packages → manifest → signed → served from loopback → editor finds it → updater installs → `--version --json` matches. Runs locally on Windows and Linux |
| Real GitHub (phase 8) | one real `0.2.0-alpha.1` on the Development channel, updated *to* from a locally installed build, on Windows and Linux |

## 11. Implementation roadmap

Each phase is one or two PRs, with tests, and waits for the previous one to merge. Nothing touches GitHub Actions until phase 4, and phase 4's first real run is announced to Paul before it starts.

| Phase | Delivers | Needs |
|---|---|---|
| 3. Version infrastructure | `crates/release` (versions, channels, provenance build script, records, validation, history queries, CLI `check`/`prepare`/`query`); `Cargo.toml` version becomes the product version; `--version [--json]` on editor, player, server; About and start page show channel and commit; `releases/` with the seeded history (D12) | Cloud; one Windows build check on Paul's PC |
| 4. Release automation | `release.yml` rewritten as §5 (validate first, profile by channel, update payload, manifest, history, signature, draft); `ci.yml` to manual only; `install.json` in packages; runbook and release checklist in `docs/project/releases.md`. Local tests first, then one `dry_run` dispatch (~1 Linux minute) | Paul's go before the first real tag; signing key from Paul |
| 5. Version Manager in the editor | Settings > Updates, update checks, notifications, Version Explorer window, Copy diagnostics | Cloud; drawing tests with snapshots |
| 6. Updater | `axcenity-updater`, download/verify/stage in the editor, swap, verify, rollback, logs | Windows and Linux runs on real machines |
| 7. Compatibility and migration | Project format 5, the migration dialog, validation, compatibility classification | Cloud, then Paul's real projects as a check |
| 8. End to end and docs | The end-to-end test, the first real release on the Development channel, user and developer docs | Paul publishes the draft |

## 12. Not proposed now

- An update server, CDN or website: GitHub Releases cover it.
- Delta (binary diff) updates: full payloads are tens of megabytes; revisit if download size becomes a complaint.
- Scheduled nightly builds: cost.
- Sigstore / GitHub artifact attestations: not available to a private repository on a personal plan; reconsider if D1 makes the repository public.
- Reproducible builds verified byte for byte: provenance is recorded and marked "not verified"; verifying needs a second build per release (double the cost).
- macOS in-app updates and OS code signing: after Windows and Linux updates work, and when certificates are bought.
- A read-only project mode for opening newer or unmigrated projects.
