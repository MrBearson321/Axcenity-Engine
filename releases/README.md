# Release files

The structured history of Axcenity: what each release changed, which features exist and where they came from, and the known issues. Release notes, the published manifest, the editor's Version Explorer and the MCP toolkit are all generated from these files, so write each fact here once. Design: [docs/research/release-system](../docs/research/release-system/README.md). Cutting a release: [docs/project/releases.md](../docs/project/releases.md).

```
releases/
  unreleased/      one file per change not released yet
  history/         one file per release: <version>.toml
  features/        the feature registry, one file per subsystem
  issues.toml      known issues
  formats.toml     the file formats the code writes now
```

Check everything with `cargo run -p axcenity-release -- check`. Unknown fields are errors, so typos don't silently drop information.

## In a pull request: add a change entry

If users would notice the change, add `releases/unreleased/<PR number or short name>.toml`:

```toml
category = "fix"            # breaking | feature | improvement | performance | fix | deprecation | security | internal
subsystem = "renderer"      # renderer physics animation terrain materials assets audio ui scripting editor
                            # project networking player gameplay navigation build toolbox release
title = "Shadows no longer flicker at dawn"
details = ""                # optional; required for `breaking`: what breaks and what to do
features = ["renderer.global-illumination"]  # feature IDs from features/ this changes
fixes = ["AX-0003"]         # issue IDs from issues.toml this fixes
introduces_issues = []      # issue IDs it's known to introduce
prs = [123]
commits = []                # for work that didn't go through a PR
stability = "stable"        # "experimental": listed as experimental, never as a finished feature
public = true               # false: kept for developers, never published
evidence = "verified"       # "reconstructed" only for history written afterwards (needs prs or commits)
```

- Write the title for a user, not a developer: what they can now do, or what no longer goes wrong.
- `internal` (refactors, build plumbing) and `public = false` entries are never published.
- Unfinished work is `stability = "experimental"` or `internal`, not a `feature`.

## A new feature: add it to the registry

In `features/<subsystem>.toml`:

```toml
[[feature]]
id = "terrain.studio"       # lowercase words joined by . and -
name = "Terrain Studio"
subsystem = "terrain"
description = "Generate, sculpt, erode and paint terrain in the editor."
status = "active"           # active | experimental | deprecated (needs deprecated_in) | removed (needs removed_in)
docs = "docs/terrain.md"
[feature.introduced]
version = "unreleased"      # a released version, "unreleased", or "unknown"
evidence = "verified"       # verified | reconstructed | unknown (only with version "unknown")
source = "PR #123"          # what shows it
```

A feature's timeline is built from the change entries that name it, so a later change only needs `features = ["terrain.studio"]` in its entry. Rows from before this system can go in `[[feature.history]]` (`version`, `change`, `evidence`, `source`).

## A known issue

In `issues.toml`:

```toml
[[issue]]
id = "AX-0003"              # next free number
title = "Shadows flicker at dawn"
subsystem = "renderer"
status = "open"             # open | fixed | wont-fix
introduced = "unknown"      # a released version, or "unknown"
confirmed_affected = ["0.2.0-alpha.1"]  # versions someone actually checked
workaround = ""
# github = 12              # the GitHub issue number, once there is one
```

The fixed version comes from the change entry whose `fixes` names the issue; set `status = "fixed"` once that release is out. Whether a version has an issue is answered as: fixed if it's at or after the fix; confirmed if listed in `confirmed_affected`; affected by range if it's at or after `introduced`; otherwise **not investigated**, never "not affected".

## Releasing

`cargo run -p axcenity-release -- prepare 0.2.0-alpha.1` moves every `unreleased/` entry into `history/0.2.0-alpha.1.toml`, sets the version in `Cargo.toml` and `Cargo.lock`, and leaves `title` and `summary` for you to write. A published release's record never changes afterwards: a mistake is fixed by a `[[corrections]]` entry (`release`, `text`) in a later record.

## What's here so far

`0.1.0-alpha.1` and `0.1.0-alpha.2` are tags that were never published; `0.1.0-alpha.3` is the one published release, checked against the tag. Features that exist in the tree at that tag are recorded as introduced there (`verified`). Everything since then was written on 2026-11-11 from pull requests and commits and is marked `reconstructed`, with its sources.

## When a file format changes

`formats.toml` lists the formats the code writes now (project, world, built game, save, network protocol). Bump the number there with the constant; that crate's tests fail until both agree. `check` then wants a `breaking` change entry in `unreleased/` saying what an older Axcenity can no longer open, and `prepare` copies the numbers into the release record.
