# Phase 1: audit of versioning, builds, packaging and GitHub

What exists today, checked against `main` at `19152d8` (2026-11-11) and the GitHub repository through its REST API. Each finding names the file or API call it came from. Nothing was built or changed.

## 1. The version number

| Where | What it says | Source |
|---|---|---|
| Root `Cargo.toml`, `[workspace.package] version` | `0.1.0` | every crate has `version.workspace = true`, so there is already one crate version |
| Editor start page footer and About box | `env!("CARGO_PKG_VERSION")`, so `0.1.0` | `crates/editor/src/start.rs:437`, `:786` |
| Player, server, `axcenity-test` | No version shown, no `--version` flag | `crates/player/src/main.rs` usage text |
| Release tag and installer | The tag (`v0.1.0-alpha.3`), passed by `release.yml` as `VERSION` to NSIS, nfpm and dmgbuild | `.github/workflows/release.yml` job `version` |
| Release docs | "The crate version in `Cargo.toml` stays `0.1.0` until 0.1 is stable. The tag is the product version." | `docs/project/releases.md` |

**Finding 1.1: two version sources disagree.** The installed `0.1.0-alpha.3` build calls itself `0.1.0` in its About box, while its installer, its uninstall entry (`DisplayVersion`) and its GitHub release say `0.1.0-alpha.3`. Nothing checks that the tag and the code agree. This is the "inconsistent version numbers" problem the brief names, and it already exists.

**Finding 1.2: no build provenance.** No build script records the commit, tag, build time, toolchain or profile. A bug report can't say which commit a build came from. The four `build.rs` files (`assets`, `project`, `cook`, `workspace-hack`) embed templates and the importer lockfile, not build information.

**Finding 1.3: no channel concept in code.** The release workflow derives "prerelease" from an `-alpha|-beta|-rc` suffix for GitHub's pre-release flag. Nothing in the engine knows about channels.

## 2. Release history

| Tag | Commit | Date | GitHub release | Release workflow run |
|---|---|---|---|---|
| `v0.1.0-alpha.1` | `2e80234c9` "Project Migration" | 2026-10-20 | **none** | none (the workflow didn't exist yet) |
| `v0.1.0-alpha.2` | `b59d2bf4d` "Add GitHub alpha releases…" | 2026-10-21 | **none** | failed |
| `v0.1.0-alpha.3` | `a897631ff` "Fix Windows and Linux release packaging…" | 2026-10-21 | **published pre-release**, 7 packages + `SHA256SUMS.txt` | success |

Sources: `git ls-remote --tags`, `GET /repos/…/releases`, `GET /repos/…/actions/runs`.

- `v0.1.0-alpha.3` is the **only published release**. It is 49 commits from the first commit, with 14 crates and project format 2.
- Since then `main` has **458 more commits** and about 93 merged pull requests (#1–#94, 2026-10-21 to 2026-11-11), 31 crates, project format 4 and network protocol 10. None of it is released.
- There is **no changelog** file anywhere in the repository. The release notes are one template (`packaging/release-notes.md.tpl`) with install instructions and no list of changes.
- Change information lives in: pull request descriptions, `docs/research/engine-modernization-report.md` (roadmap IDs I1…, M1…, L1…), the phase plans in `plans/`, `docs/benchmarks.md`, and commit messages (early ones are short: "Added remaining features").
- Issues: the repository has **no GitHub issues** (only pull requests). Labels are GitHub's defaults (`bug`, `enhancement`, …); no PR uses them for release categories.

**Finding 2.1:** the Version Explorer can be *verified* for one release (alpha.3: its tree at the tag is the evidence of what it contained). Everything since is unreleased and can only be *reconstructed* from PRs and roadmap docs, which is how it must be labelled.

## 3. Build and packaging

| Piece | State | Source |
|---|---|---|
| Toolchain | Rust 1.97.0 pinned in `rust-toolchain.toml`, repeated in both workflows | |
| Profiles | `release` (thin LTO), `dist` (fat LTO, 1 codegen unit; what releases ship) | `Cargo.toml` `[profile.*]` |
| Staging | `packaging/stage.py`: one payload per OS (editor, player, `players/<os>/` for Build Game, licences) | |
| Packages | Windows NSIS setup `.exe`; macOS universal `.dmg`; Linux `.deb`, `.rpm`, `.pkg.tar.zst`, `.tar.gz`, `.AppImage` | `docs/project/releases.md` |
| Local check | `scripts/check-packages.py` builds, packages, runs Build Game and starts the game; verified on Windows, macOS and Linux (2026-11-10/11) | PR #62, #81 |
| Checksums | `SHA256SUMS.txt` beside the packages | `release.yml` job `publish` |
| Signing | None. SmartScreen and Gatekeeper warn | `releases.md` |

## 4. GitHub configuration

- **The repository is private** (`GET /repos/MrBearson321/Axcenity-Engine`: `"private": true`). This matters more than anything else in this audit:
  - The engine can't read releases or download their files without a GitHub token. An unauthenticated update check gets `404`.
  - Actions minutes on a private repository are billed against the account's allowance; public repositories use standard runners free.
  - GitHub's artifact attestations (Sigstore build provenance) aren't available to private repositories on personal plans.
- Workflows: `CI` (`ci.yml`, on every push to `main` and every PR) and `Release` (`release.yml`, on `v*` tags and manual dispatch) are both `disabled_manually`. The only active workflow is GitHub's Copilot agent.
- `release.yml` already does most of the build: a version check, builds on Windows, macOS (two targets, joined with `lipo`) and Ubuntu 22.04, three packaging jobs, then `gh release create` with all files. It publishes straight away; there is no draft step, no validation of notes, and no machine-readable manifest.
- **Cost of the one successful run** (alpha.3, 2026-10-21, from the jobs API): Linux build 12 min, Windows build 21 min, macOS build 11 min, packaging 1–2 min each, publish under 1 min. GitHub bills Windows minutes at about 2× and macOS at about 10× Linux, so that run cost roughly **185 Linux-minute equivalents, about 60% of it macOS**. The code base has grown from 14 to 31 crates since, so a full run today is likely 2–3× that. Check the account's current plan and rates before relying on these numbers.

Paul's decision of 2026-11-11: Actions will be turned on, with cost efficiency first, no frequent releases, workflows tested before they run, and Paul told before any run publishes a release.

## 5. Supported platforms and install locations

| Package | Installs to | Who can write there | In-app update possible? |
|---|---|---|---|
| Windows NSIS setup | `%LOCALAPPDATA%\Programs\Axcenity Engine`, per user, `RequestExecutionLevel user`, uninstall key under `HKCU` | the user | Yes |
| Linux `.tar.gz` | wherever it's extracted | the user | Yes |
| Linux AppImage | one file, wherever it's kept | the user | Yes |
| Linux `.deb` / `.rpm` / `.pkg.tar.zst` | `/opt/axcenity`, links in `/usr/bin` | root, owned by the package manager | No: replacing those files behind the package manager's back would corrupt its database |
| macOS `.dmg` | `/Applications/Axcenity Editor.app` | admin users | Technically, but the app is unsigned and replacing it trips Gatekeeper quarantine checks |

Primary targets per the [development policy](../../development-policy.md): Windows and Linux. macOS is built and shipped but secondary.

## 6. User data and where it lives

The engine already keeps user data out of the install folder, which is what the updater needs:

| Data | Location | Source |
|---|---|---|
| Editor settings, layout, recent projects | the OS's app-data folder for `Axcenity Editor` (`eframe::storage_dir(APP_ID)`; under `%APPDATA%` on Windows, `~/.local/share` on Linux) | `crates/editor/src/main.rs` `APP_ID` |
| Editor plugins (global) | same storage folder | `crates/editor/src/plugins.rs:29` |
| Global Assets | same storage folder, `Global Assets/` | `crates/editor/src/global_assets.rs:26` |
| Rebuild-and-relaunch handoff | same storage folder, `Relaunch/` | `crates/editor/src/relaunch.rs:137` |
| Game saves | `%APPDATA%\Axcenity\Saves\<game>`, `~/Library/Application Support/Axcenity/Saves`, `$XDG_DATA_HOME/axcenity/saves` | `crates/save/src/lib.rs:573` |
| Toolbox central asset directory and `metadata.db` | its own directory (`AXCENITY_ASSET_DIR`) | PR #96, `crates/toolbox` |
| Projects | wherever the user keeps them | |

**Finding 6.1:** nothing in the install folder is user data. An updater that replaces only the install folder, and never follows paths outside it, can't touch projects, settings, plugins or assets.

## 7. Format versions that already exist

These are separate compatibility boundaries, each with its own constant. None is tied to the engine version, which is what the brief asks for.

| Format | Constant | Value | Old versions handled by | Source |
|---|---|---|---|---|
| Project | `axcenity_project::FORMAT_VERSION` | **4** (was 2 at alpha.3) | `migrate.rs`: steps 1→2→3→4, copy kept in `.axcenity/previous/<n>/`; newer refused with "update Axcenity to open it" | `crates/project/src/lib.rs:143` |
| World file | `WORLD_FORMAT_VERSION` | 1 | | `crates/project/src/world_file.rs:84` |
| Game saves | `SAVE_VERSION` | 2 | `crates/save/src/migrate.rs` | `crates/save/src/lib.rs:39` |
| Built game `.axgame` | `packed::VERSION` | 2 (M9) | v1 still unpacks | `crates/project/src/packed.rs:16` |
| Network protocol | `PROTOCOL_VERSION` | 10 (M7, #34) | refused with both numbers named | `crates/net/src/lib.rs:48` |
| Relaunch handoff | `HANDOFF_VERSION` | 1 | only the same version is read | `crates/editor/src/relaunch.rs:28` |
| Asset IDs / reference index | `FORMAT_VERSION`, `INDEX_VERSION` | 1, 1 | | `crates/assets/src/asset_id.rs:41`, `references.rs:27` |
| Toolbox `metadata.db` | `SCHEMA_VERSION` | 1 | newer refused | PR #96 `crates/toolbox/src/db.rs` |
| Toolbox asset record | `format_version` | 1 | readers ignore unknown fields | `docs/research/asset-sources/provider-interface.md` |
| Editor settings (eframe RON) | none | | | |

**Finding 7.1: the project upgrade is automatic.** Opening a format-2 project in today's editor rewrites it to format 4 on open, keeps a copy in `.axcenity/previous/2/`, and reports it in Output afterwards. The brief asks for the user to be told and to agree first.

**Finding 7.2: projects don't record which engine wrote them.** The manifest (`axcenity.json` → `Manifest`) has `format_version` and the game's own `version`, not the engine's. It is `#[serde(deny_unknown_fields)]`, so adding a field is itself a format change: an older engine at the same format number would refuse the file.

**Finding 7.3: name clash.** "Version History" already means a project's saved checkpoints (Phase 72, `crates/project/src/versions.rs`, File > Version History). The engine's release history needs a different name in menus; "Version Explorer" (the brief's name) is fine as long as it isn't shortened to "Version History".

## 8. Libraries already in the tree

What an updater would need, and whether it's already a dependency (`Cargo.lock`):

| Need | Already there | Would be new |
|---|---|---|
| HTTPS client | `ureq` 3 with rustls (Toolbox, Get texture) | |
| JSON | `serde_json` | |
| TOML for hand-written release records | `toml_edit`, `toml_parser` (indirect) | `toml` as a direct dependency |
| SemVer parsing and precedence | `semver` 1.0 (indirect) | as a direct dependency |
| zip / tar.gz extraction | `zip` 2, `flate2` (indirect) | `tar` |
| SHA-256 | no (`ring` is in the tree via rustls but isn't a convenient API; Toolbox adds `md-5`) | `sha2` |
| Signature verification | no | `minisign-verify` (pure Rust, no dependencies, MIT) |
| SQLite | `rusqlite` (Toolbox, PR #96) | not needed here: the history is small |

## 9. What the updater and distribution need (summary)

1. One version, compiled in, matching the tag, with a channel derived from it.
2. Release information the engine can read **without a token**, which a private repository can't give (decision D1).
3. A machine-readable, signed release manifest; today there are only package files and `SHA256SUMS.txt`.
4. An update payload per platform that can be swapped in place: the staged folder the installers already contain, zipped.
5. A way to know how the running copy was installed, since only some installs can update themselves (section 5).
6. A project-compatibility answer per release (the format table in section 7, recorded with each release).
