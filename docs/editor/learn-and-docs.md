# Learn, Documentation and the API pages

The start screen's **Learn** and **Documentation** pages (and the same pages in Help > Learn) and the published documentation site are one set of Markdown files in `documentation/`. The **API Reference** page and `documentation/api/` come from the script crate's API description.

## Files

- `documentation/SUMMARY.md`: the table of contents (GitBook's format). A `## ` section whose title starts with `Learn` (`docs::LEARN_SECTION`) is a group of chapters on the Learn page, titled after `Learn:`; the others are the Documentation page's contents. The `## API reference` section to the end is generated.
- `documentation/learn/*.md`: the learning path, one chapter per part of the engine. `documentation/reference/*.md`: reference pages. `documentation/README.md`: the Documentation page's Overview.
- `documentation/api/`: generated (below); the editor doesn't embed it, since the API Reference page shows the same entries.
- Front matter between `---` lines: `description` (required), `icon` (a name from `docs::markdown::icon`), `status` (`stable`, `partial` shown as "Partly built", `experimental`) and `samples: [a, b]` (folders under `examples/`; Learn shows them with Open sample and the tutorial that uses each).

## Code

- `crates/editor/build.rs` embeds every `.md` under `documentation/` but `api/` as `DOC_FILES` (`OUT_DIR/doc_files.rs`), so the pages ship inside the editor and work offline.
- `start/docs/library.rs`: the pure parser. `Library::load` reads `SUMMARY.md` and the pages into `Page`s of `Block`s (headings with GitHub-style anchors, paragraphs, lists, code, `>` notes, pipe tables, rules) and `Span`s (text, bold, code, links); `resolve` turns a link into a page and anchor, a sample (`.../examples/<name>`), an API entry (`api/...`), a URL or Missing; `search` scores titles over headings over text. Anything it can't show (nested lists, HTML, images, GitBook `{% %}` blocks, unclosed `**` or backticks) is a page problem.
- `start/docs/markdown.rs`: draws a page with theme tokens. Code spans that name an API entry (`api_name`: `Part.Touched`, `Instance:FindFirstChild()`, `task.wait()`, `workspace:Raycast()` through `global_class`) are links to it; code blocks have Lua colours and Copy. `pages_mentioning` lists the pages whose code names an entry (the API Reference's "Read more").
- `start/docs.rs`: the Documentation page (search, a contents column wider than `TWO_COLUMNS`, a Contents fold narrower, Back/Next). `start/learn.rs`: Learn's chapter cards (`chapters`) above the tutorials; a chapter page adds "Try it in a sample" and Back/Next chapter. Clicks become `DocAction`s that `StartScreen::follow_doc` routes: a Learn chapter opens in Learn, any other page in Documentation, an API name in the API Reference.
- `start/reference.rs`: an entry's page shows `axcenity_script::api_example` under Example (with **Insert into <file>** while a Lua tab is active; `docs/editor/scripts.md`) and the "Read more" pages. The same examples show in code tabs' hover docs.

## API reference generation

`crates/script/src/definitions/` describes the API once. From it come `lua_definitions` (`definitions/axcenity.d.luau`), `api_reference` (the editor page), `api_markdown` (`documentation/api/`: one page per class and type, one each for enums, libraries and globals) and `api_summary` (SUMMARY's API section). Usage examples are `definitions/examples.rs`. `cargo run -p axcenity-script --bin lua-definitions` writes all of them; `api_pages_are_up_to_date` and `definitions_file_is_up_to_date` fail until it's run.

## Tests

- Editor `start::docs::tests`: every page reads without problems and has a description and known icon; every link, anchor and sample resolves; every sample opens; every `Owner.Member` in backticks with a known owner exists in the API (renaming an API member fails here); every Lua block compiles (`axcenity_script::syntax_error`) and every block names a known language; the learning path covers the brief's topics; every page draws wide and narrow (with `AXCENITY_SNAPSHOTS`, `docs-<path>.png`); search draws.
- Editor `script_completion::lint::tests::every_api_example_passes_the_editors_own_checks`: no example (115, scripting priority 2) has an error or warning from the editor's own API checks, except naming objects an empty world lacks.
- Script `definitions::tests`: `every_example_names_a_real_entry_and_compiles`, `api_pages_are_up_to_date`, `the_api_summary_replaces_only_its_own_section`.

## Writing pages

Document only what exists; mark partial or experimental systems with `status:` and say what's missing in a **Known limits** note. Use the editor's own words for menus and buttons. When a feature changes, change its page in the same pull request. The user-facing version of this file is `documentation/reference/maintaining-docs.md`.
