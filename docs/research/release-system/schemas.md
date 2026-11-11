# Release data: files, schemas and the published manifest

The concrete shapes behind [architecture.md](architecture.md) sections 3 and 9. Everything a person writes is TOML in `releases/` (comments and multi-line text are easier there than in JSON). Everything the engine reads is JSON generated from it. No fact is written in two places: what a developer knows goes in `releases/`, what only the build knows (commit, checksums, toolchain) is added by the release workflow.

## Repository layout

```
releases/
  README.md                 how to add a change entry (one screen)
  unreleased/               one file per change, usually one per pull request
    0096-toolbox-steps.toml
  history/                  one file per release, written by `axcenity-release prepare`
    0.1.0-alpha.3.toml
    0.2.0-alpha.1.toml
  features/                 the feature registry, one file per subsystem
    renderer.toml
    physics.toml
    …
  issues.toml               known issues
```

`axcenity-release` is the proposed crate (`crates/release`, a library plus a small command-line program) that parses, validates and queries these files. The editor, the updater, the release workflow and the MCP toolkit all go through it, so there is one parser and one set of rules.

## A change entry (`releases/unreleased/*.toml`)

Written in the same pull request as the change. One small file per change means two PRs never conflict over a shared changelog.

```toml
category = "performance"   # feature | improvement | performance | fix | breaking | deprecation | security | internal
subsystem = "physics"      # one of the subsystems listed below
title = "Large worlds step physics up to 39% faster"
details = """
Optional. Longer explanation for the release notes: what changed for the
user, what to do about it.
"""
stability = "stable"       # stable | experimental. Experimental entries are listed under
                           # "Experimental", never as finished features.
features = ["physics.rigid-bodies"]  # feature IDs this changes (adds history rows to them)
fixes = []                 # issue IDs this fixes (sets their fixed_in at release)
introduces_issues = []     # issue IDs this is known to introduce
prs = [84]
public = true              # false: recorded for developers, never published
```

Rules the validator enforces:

- `category = "internal"` or `public = false` is never published, in notes or history.
- `breaking` needs `details` that say what breaks and what to do.
- A change of any format constant (table in [audit.md](audit.md#7-format-versions-that-already-exist)) since the last release needs at least one `breaking` or `improvement` entry naming that format; `prepare` refuses otherwise.
- Feature and issue IDs must exist in `features/` and `issues.toml`.

Subsystems (the Version Explorer's filters): `renderer`, `physics`, `animation`, `terrain`, `materials`, `assets`, `audio`, `ui`, `scripting`, `editor`, `project`, `networking`, `player`, `build`, `toolbox`, `release`.

## A release record (`releases/history/<version>.toml`)

`axcenity-release prepare <version>` creates it: it moves every file from `unreleased/` into the record's `changes`, reads the format constants from the code, and leaves the title and summary for a person to write. After the release is tagged, the record doesn't change, except through an explicit `[[correction]]` entry (see [architecture.md](architecture.md#published-releases-are-immutable)).

```toml
version = "0.2.0-alpha.1"
date = "2026-11-20"
title = "Multiplayer, the Toolbox and faster physics"
summary = """
Two to four sentences for the update notification and the top of the notes.
"""
evidence = "verified"      # verified: made by this system | reconstructed: written afterwards from PRs and tags

[formats]                  # filled in by `prepare` from the code's constants
project = 5
world = 1
save = 2
axgame = 2
network_protocol = 10
toolbox_database = 1
asset_record = 1

[compatibility]
upgrades_projects_from = 1 # oldest project format this release can migrate
update_protocol = 1        # how this editor talks to an updater (see architecture.md)

[verification]             # what was run on real machines before tagging (development policy rule 5)
windows = "check-platform.ps1, check-packages.py: pass, 2026-11-19"
linux = "check-platform.sh, check-packages.py: pass, 2026-11-19"

[[changes]]                # one per former unreleased/ file, kept as written
category = "performance"
subsystem = "physics"
title = "Large worlds step physics up to 39% faster"
features = ["physics.rigid-bodies"]
prs = [84]
```

The channel is not written: it follows from the version (`-alpha.N` development, `-beta.N` and `-rc.N` beta, no suffix stable), so it can't disagree with it.

## The feature registry (`releases/features/<subsystem>.toml`)

Identity and status of each feature. Its history is mostly *derived* from the change entries that name it, so it isn't written twice. Rows before this system existed are written here by hand, each with its evidence.

```toml
[[feature]]
id = "terrain.editor"
name = "Terrain Studio"
subsystem = "terrain"
description = "Sculpt, paint and generate terrain in the editor."
status = "active"          # active | experimental | deprecated | removed
docs = "docs/terrain.md"

[feature.introduced]
version = "0.1.0-alpha.3"
evidence = "verified"      # verified | reconstructed | unknown
source = "crates/terrain exists in the tree at tag v0.1.0-alpha.3"

[[feature.history]]        # only for rows not covered by a change entry
version = "unreleased"
change = "Foliage painting"
evidence = "reconstructed"
source = "PR #83"
```

`deprecated` needs `deprecated_in` and a replacement or reason; `removed` needs `removed_in`.

## Known issues (`releases/issues.toml`)

```toml
[[issue]]
id = "AX-0001"             # stable ID; `github` links the GitHub issue once there is one
title = "Translucent leaves render black with the sun behind them on some GPUs"
subsystem = "renderer"
status = "open"            # open | fixed | wont-fix
introduced = "unknown"     # a version, or "unknown"
confirmed_affected = ["0.2.0-alpha.1"]   # versions someone actually checked
fixed_in = ""              # set by `prepare` from change entries' `fixes`
workaround = "Turn off leaf translucency in Settings > Graphics."
github = 0
prs = []
public = true
```

Whether version *V* has the issue, answered in this order:

1. `fixed_in` is set and *V* ≥ `fixed_in`: **not affected (fixed)**. A fix that is still unreleased counts for a *V* newer than every release: that is a build of the coming release.
2. *V* is in `confirmed_affected`: **affected (confirmed)**.
3. `introduced` is a version and `introduced` ≤ *V* (and *V* < `fixed_in` if set): **affected (by version range, not checked on V)**.
4. Otherwise: **not investigated**. Never "not affected".

## The published manifest (`axcenity-release.json`)

Generated by the release workflow from the release record plus what the build produced, attached to the GitHub release with its signature `axcenity-release.json.minisig`. This is the only file the engine trusts directly; everything else it downloads is checked against a hash in here.

```json
{
  "schema": 1,
  "product": "axcenity-engine",
  "version": "0.2.0-alpha.1",
  "channel": "development",
  "tag": "v0.2.0-alpha.1",
  "commit": "0f3c…(40 hex)",
  "published": "2026-11-20T18:04:11Z",
  "title": "Multiplayer, the Toolbox and faster physics",
  "summary": "…",
  "notes": {
    "features": [], "improvements": [], "performance": [
      { "subsystem": "physics", "title": "Large worlds step physics up to 39% faster", "details": "", "prs": [84] }
    ],
    "fixes": [], "breaking": [], "deprecations": [], "security": [], "experimental": []
  },
  "known_issues": ["AX-0001"],
  "formats": { "project": 5, "world": 1, "save": 2, "axgame": 2, "network_protocol": 10, "toolbox_database": 1, "asset_record": 1 },
  "compatibility": { "upgrades_projects_from": 1, "min_update_protocol": 1 },
  "artifacts": {
    "windows-x86_64": {
      "update":    { "file": "AxcenityEditor-0.2.0-alpha.1-windows-x86_64-update.zip", "sha256": "…", "size": 123456789 },
      "installer": { "file": "AxcenityEditor-0.2.0-alpha.1-windows-x86_64.exe", "sha256": "…", "size": 123456789 }
    },
    "linux-x86_64": {
      "update":    { "file": "AxcenityEditor-0.2.0-alpha.1-linux-x86_64.tar.gz", "sha256": "…", "size": 0 },
      "appimage":  { "file": "AxcenityEditor-0.2.0-alpha.1-linux-x86_64.AppImage", "sha256": "…", "size": 0 },
      "deb":       { "file": "…", "sha256": "…", "size": 0 }
    }
  },
  "history": { "file": "axcenity-history.json", "sha256": "…", "size": 0 },
  "provenance": {
    "workflow_run": "https://github.com/…/actions/runs/…",
    "rustc": "1.97.0", "profile": "release", "runner_images": { "windows-x86_64": "windows-2022 …", "linux-x86_64": "ubuntu-22.04 …" },
    "source_date_epoch": 1763661600,
    "reproducible": "not verified"
  }
}
```

Notes on the shape:

- **Artifacts are named, not linked.** The engine builds each download URL itself from its compiled-in release source, the tag and the file name. A tampered manifest can't point a download at another host, and the signature covers the names and hashes.
- `channel` is stored for readers, but the engine recomputes it from `version` and refuses a manifest where they differ.
- `reproducible` stays `"not verified"` until a second build from the same tag has been compared byte for byte. Recording build metadata is not the same as a reproducible build, and the manifest says which one it is.
- Unknown fields are ignored by readers; `schema` goes up only when a field changes meaning (the rule the Toolbox's asset record uses too).

## The history file (`axcenity-history.json`)

Every public release record, feature and issue, compiled by `axcenity-release history` and attached to each release. Its SHA-256 is in the signed manifest. The editor also has the history compiled in at build time (from `releases/`), so the Version Explorer works offline and on a build that has never been online; a newer downloaded history replaces it in the cache.

It is the same data model the editor and the MCP toolkit query, serialized. Internal entries (`public = false`, `category = "internal"`) are dropped before it is written, and a test checks that none survive.
