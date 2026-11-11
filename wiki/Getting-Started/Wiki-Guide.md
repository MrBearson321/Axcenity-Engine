---
title: Wiki guide
type: index
status: current
verified: 2026-11-10
tags: [meta]
---

# Wiki guide

How this wiki is organized, how to write a page, and how to keep it true. Read this before adding or changing pages.

## What the wiki is for

Axcenity's persistent engineering memory: **what exists, why, how it works, how it performs, and how it should evolve**. It sits beside two other layers and does not replace them:

| Layer | Holds | Authority |
|---|---|---|
| **Source and tests** (`crates/`) | What the engine actually does | **Final.** When anything disagrees with the code, the code wins, and the doc gets fixed |
| **Topic docs** (`docs/`, crate `CLAUDE.md`) | One short file per kind of change: behavior, rules, names | Kept in step with every change by the repo's rules |
| **This wiki** | Architecture, traced workflows, decisions (ADRs), performance history, limitations, guides | Explains and connects; links to the two layers above for detail |

So a system page says *how the renderer is put together and why*, and links to `docs/rendering/water.md` for *what a water material's fields do*. If you find yourself copying a paragraph from `docs/`, link to it instead ([ADR-0012](../Architecture/Architecture-Decisions/ADR-0012-Wiki-Complements-Topic-Docs.md)).

## Layout

| Folder | Contents |
|---|---|
| `Home.md` | Entry point and system directory |
| `Getting-Started/` | This guide, engine overview, build and run |
| `Architecture/` | Overview, repository structure, dependencies, lifecycle, threading, memory, platforms, the system inventory, and `Architecture-Decisions/` (ADRs) |
| `Engine-Systems/` | One page per engine system |
| `Editor/` | Editor architecture and the specialized editors |
| `Asset-Pipeline/` | Import, cooking, loading, formats |
| `Performance/` | Overview, benchmarking, profiling, optimization log, benchmark history, known bottlenecks, hardware notes |
| `Development/` | Coding standards, implementation patterns, testing, debugging, contribution workflow |
| `Guides/` | Step-by-step recipes and troubleshooting |
| `Reference/` | Dependencies, file formats, configuration, terminology |
| `Roadmap/` | Roadmap status, known limitations, technical debt |
| `Changelog/` | Engineering changelog and documentation history |
| `Templates/` | Page templates (hidden from search and the graph) |

## Writing a page

1. Start from a template in `Templates/` (Obsidian: *Insert template*).
2. **Describe the actual implementation.** Read the source before writing. Never describe planned work as if it exists; put it under a "Planned" heading or in [Current roadmap](../Roadmap/Current-Roadmap.md).
3. **Name the source.** Use repo-relative paths in backticks (`crates/sim/src/steps.rs`) and symbol names (`Schedule`, `ScriptRuntime::step`). Paths are checked by the validator, so keep them exact. Prefer a file to a line number; line numbers rot.
4. **Mark evidence on performance claims**: **Measured** (machine, method, link to the table), **Expected** (reasoned from a measurement), **Theoretical**, or **Unverified**. Never replace an old number; add a dated row ([Benchmark history](../Performance/Benchmark-History.md)).
5. **Separate machine-specific observations** (the 9950X3D's CCD pinning, an OLED's VRR flicker) into [Hardware notes](../Performance/Hardware-Notes.md).
6. **Link generously** between wiki pages with relative Markdown links: `[Physics](../Engine-Systems/Physics.md)`. Obsidian tracks them for backlinks and the graph, and they also work on GitHub, in VS Code and for agents. No `[[wikilinks]]`, no spaces in file names.
7. Use Mermaid for diagrams. Keep each one small enough to read; one idea per diagram.

## Frontmatter

| Field | Meaning |
|---|---|
| `type` | `system`, `architecture`, `adr`, `guide`, `reference`, `performance`, `roadmap`, `index` |
| `status` | `current`, `draft`, `superseded` (of the page itself) |
| `implementation` | System pages: `implemented`, `partial`, `experimental`, `planned`, `deferred`, `deprecated` |
| `source`, `docs` | Repo paths the page describes. The validator checks they exist and flags the page when they change after `verified_commit` |
| `verified`, `verified_commit` | When and at which commit the page was last checked against the source |
| `adr_status` | ADRs: `Proposed`, `Accepted`, `Superseded`, `Rejected` |
| `tags` | `system/<name>`, `perf`, `adr`, `guide`, … |

## Keeping it true

| When | Do |
|---|---|
| A change alters documented behavior, architecture, an API, the frame order, a dependency, a file format, or performance | Update the system page in the same pull request; bump `verified` and `verified_commit` |
| An optimization lands | Add an entry to the [Optimization log](../Performance/Optimization-Log.md) and rows to [Benchmark history](../Performance/Benchmark-History.md) |
| A decision with long-term consequences is made or reversed | Write or supersede an ADR; never edit an accepted ADR's decision in place |
| A limitation is found or fixed | Update [Known limitations](../Roadmap/Known-Limitations.md) or [Technical debt](../Roadmap/Technical-Debt.md) |
| A roadmap item moves | Update [Current roadmap](../Roadmap/Current-Roadmap.md) |
| Any wiki edit | One line in [Documentation history](../Changelog/Documentation-History.md); run the validator |

Trivial changes (a renamed local, a test, a tooltip) don't need a wiki edit.

### The validator

`python scripts/check-wiki.py` (from the repository root; `--wiki <folder>` if the vault is elsewhere) checks:

- every relative link between pages resolves, including `#heading` anchors;
- every backticked repo path (`crates/…`, `docs/…`, `plans/…`, `scripts/…`, `benchmarks/…`) and every `source`/`docs` frontmatter path exists;
- pages whose `source` files changed since their `verified_commit` (reported as "may be stale");
- ADR numbers are unique and every ADR is in the ADR index;
- pages no other page links to (orphans).

It reads only; it never edits a page.
