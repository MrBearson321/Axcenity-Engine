# Agent instructions for Axcenity Engine

For every AI coding agent (Claude Code, Cursor, Codex and others). The project's rules, invariants and commands are in [CLAUDE.md](CLAUDE.md); read it first. This file adds how to use and maintain the **engineering wiki**, an Obsidian vault of Markdown in [wiki/](wiki/Home.md).

## What to trust

1. **Source and tests** are the final authority on what the engine does.
2. **Topic docs** (`docs/`, mapped by [docs/README.md](docs/README.md)) and crate `CLAUDE.md` files describe behavior, one file per kind of change.
3. **The wiki** explains architecture, traces workflows, records decisions (ADRs), keeps performance history and lists limitations. It links to the two layers above instead of copying them.

If the wiki or a doc disagrees with the code, investigate; don't follow either blindly. Fix whichever is wrong in the same change.

## Before changing code

For anything beyond a trivial edit:

1. **Name the affected systems.** [wiki/Architecture/System-Inventory.md](wiki/Architecture/System-Inventory.md) maps every system to its crate, status and pages.
2. **Read the system's wiki page** (`wiki/Engine-Systems/…`, `wiki/Editor/…`, `wiki/Asset-Pipeline/…`) and the **ADRs** it links ([index](wiki/Architecture/Architecture-Decisions/ADR-Index.md)).
3. **Open the one topic file** for the change (from `docs/README.md`) and the crate's `CLAUDE.md`.
4. **Read the source and its tests.** Follow the existing patterns ([wiki/Development/Implementation-Patterns.md](wiki/Development/Implementation-Patterns.md)).
5. **Check it doesn't exist already.** Prefer extending an existing system to building a parallel one.
6. **Check side effects**: crate edges ([wiki/Architecture/System-Dependencies.md](wiki/Architecture/System-Dependencies.md)), the frame order, threads, file formats and versions, the network protocol, the player/server staying free of editor, GPU or audio code.
7. **For performance work**, read [wiki/Performance/Performance-Overview.md](wiki/Performance/Performance-Overview.md), [Known bottlenecks](wiki/Performance/Known-Bottlenecks.md) and the [Optimization log](wiki/Performance/Optimization-Log.md) first: several ideas were already measured, rejected or parked.

For a major proposal or plan, cite the wiki pages and ADRs it relies on.

## While implementing

- Use the documented architecture and patterns; preserve behavior and compatibility unless the task says otherwise.
- Think about performance, memory, threading (the World is single-threaded; workers never touch it), error handling, resource lifetimes, API compatibility and tests.
- **Never silently override an accepted ADR.** If the change conflicts with one, say so, explain the conflict, and either fit the change to the decision or propose a superseding ADR for the owner to accept.
- Don't change code just because documentation suggests an improvement; that still needs normal planning, tests and verification.
- Follow the [development policy](docs/development-policy.md): desktop first, no infrastructure before it's needed, measure before optimizing, local tests for significant changes (CI is off on purpose).

## After implementing

Update the wiki **in the same change** when it materially affects what's documented:

| Change | Update |
|---|---|
| New system, architectural change, new API, changed execution flow or frame step | The system page (and diagrams); `verified` and `verified_commit` in its frontmatter |
| Performance work with measurements | An entry in [Optimization log](wiki/Performance/Optimization-Log.md), a dated row in [Benchmark history](wiki/Performance/Benchmark-History.md), [Known bottlenecks](wiki/Performance/Known-Bottlenecks.md) if it opens or closes one |
| A decision with long-term consequences | A new ADR from `wiki/Templates/ADR.md`; supersede, never rewrite, an accepted one |
| New dependency, file format change, configuration | [Dependencies](wiki/Reference/Dependencies.md), [File formats](wiki/Reference/File-Formats.md), [Configuration](wiki/Reference/Configuration-Reference.md) |
| Important bug fix, new limitation or technical debt | [Known limitations](wiki/Roadmap/Known-Limitations.md), [Technical debt](wiki/Roadmap/Technical-Debt.md) |
| Roadmap item finished or started | [Current roadmap](wiki/Roadmap/Current-Roadmap.md) |
| Any wiki edit | A line in [Documentation history](wiki/Changelog/Documentation-History.md) |

Don't touch the wiki for trivial changes. The behavior topic file in `docs/` is still updated as `CLAUDE.md` says.

Then run `python scripts/check-wiki.py` (broken links, missing source paths, pages whose sources changed since they were verified, ADR index). Fix errors; review "may be stale" warnings for pages your change touched.

## Writing wiki pages

Follow [wiki/Getting-Started/Wiki-Guide.md](wiki/Getting-Started/Wiki-Guide.md): describe the actual implementation, never present planned work as built, name source files by repo path in backticks, mark performance claims Measured / Expected / Theoretical / Unverified with machine and method, keep machine-specific notes in [Hardware notes](wiki/Performance/Hardware-Notes.md), use relative Markdown links between pages, and keep the files portable outside Obsidian.
