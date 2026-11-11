---
description: Where these pages live, how they're checked so they stay true, how the API reference is generated, and how to add or change a page.
icon: books
---

# Keeping this documentation current

These pages are written for people making games. They describe only what the engine does today, and they're checked every time the engine's tests run, so a renamed API, a removed sample or a broken link fails a test until a page is fixed.

## Where things live

| What | Where | Made by |
|---|---|---|
| Learn chapters and reference pages | `documentation/learn/`, `documentation/reference/` | Written by hand |
| The table of contents | `documentation/SUMMARY.md` | By hand, except its API section |
| API reference pages | `documentation/api/` and the API section of `SUMMARY.md` | Generated |
| The editor's API Reference, the VS Code definitions | `definitions/axcenity.d.luau` | Generated |
| Engine internals for contributors | `docs/` topic files and the `wiki/` | Written by hand |

The same Markdown files are the in-editor **Learn** and **Documentation** pages (built into the editor) and the published documentation site. A section of `SUMMARY.md` whose title starts with `Learn` becomes a group of chapters on the Learn page; the other sections are the Documentation page's contents.

## A page

Each page starts with a short header, then one `#` title:

```text
---
description: One sentence for search results and the contents.
icon: atom
status: partial
samples: [hello-physics, hello-joints]
---

# Physics
```

- `description` is required. `icon` is one of the names the editor knows (the test lists them).
- `status` is `stable` (the default, no badge), `partial` ("Partly built") or `experimental`. Mark a page when the feature it describes is missing a part, is off by default or is still changing, and say what in a **Known limits** note.
- `samples` lists example projects under `examples/`; a Learn chapter shows them as "Try it in a sample" with an **Open sample** button.

Pages use plain Markdown: headings, paragraphs, bold, `code`, links, lists, numbered lists, `>` notes, tables and fenced code blocks with a language (`lua`, `json`, `css`, `text`, `sh`, `powershell` or `toml`). Lists inside lists, HTML and images aren't shown in the editor, so the checks refuse them.

## What the checks catch

`cargo test -p axcenity-editor docs` runs these:

- Every page reads without problems, has a description and is listed in `SUMMARY.md`, and everything listed exists.
- Every link goes to a page and heading that exist, or to a sample that exists.
- Every sample a page names opens as a project.
- Every API name in backticks with a known owner, such as `Part.Touched` or `workspace:Raycast()`, exists in the API. Renaming or removing an API member fails here until the pages say so.
- Every Lua code block compiles.
- The learning path covers every part of the engine the brief asked for.
- Every page draws in the editor, wide and narrow.

## The API reference

The API is described once, in the script crate's definitions (`crates/script/src/definitions/`). From that one description come the editor's API Reference page, the VS Code definitions file, and the pages in `documentation/api/`. Usage examples for the most used entries are in `definitions/examples.rs`; each must name a real entry and compile.

After changing the script API, regenerate everything:

```sh
cargo run -p axcenity-script --bin lua-definitions
```

`cargo test -p axcenity-script` fails until the generated files match the API.

## Changing a page

1. Edit or add the Markdown file, and list a new page in `SUMMARY.md`.
2. Run `cargo test -p axcenity-editor docs` and fix what it reports.
3. To see the pages as the editor draws them, set `AXCENITY_SNAPSHOTS` to a folder and run the same tests: they save a picture of each page there.
4. When a feature changes, change the page in the same pull request.
