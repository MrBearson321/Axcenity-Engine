# Usability brief, Phase 6: codebase maintainability

Paul's usability brief (2026-11-11, §9 and Phase 6) asks for the code to become easier for developers and AI agents to read and change: find oversized files and files with unrelated responsibilities, split independent systems into modules, cut duplication and coupling, and keep public APIs and behavior unchanged, without refactoring only to reduce line counts.

This pass works **inside** the existing crates. Moving editor panel families into their own crates is G11 ([gap analysis](../engine-gap-analysis.md)), which another thread is doing (`editor-kit`, `editor-toolbox`, `editor-benchmark` are merged; Model Studio, gizmo, snapping, shortcuts and animation posing/preview are next, into `editor-tools`). Those files are left alone here.

Splits follow the repository's existing pattern ([crates.md, "Where the big files split"](../../architecture/crates.md#where-the-big-files-split)): the parent file keeps the types and entry points; each child holds one responsibility, starts with `use super::*;`, and is either another `impl` block or free functions. Code moves verbatim. The only edits are visibility (a function the parent calls becomes `pub(super)` or `pub(crate)`), the `mod`/`use` lines, and a short `//!` comment at the top of each new file saying what it holds.

## Findings

Files over ~1,500 lines were read and sorted into three kinds: several responsibilities in one file (split), one cohesive thing that is just long (left), and generated or table data (left).

| # | File | Lines | What's wrong | Change |
|---|---|---|---|---|
| R1 | `editor/src/properties.rs` | 3,105 | Four jobs: the section table (which property goes in which group, 360 lines of data), the panel itself, file-picking controls for nine kinds of file, and the value control for every type; plus 760 lines of tests inline | Split: `properties/groups.rs`, `files.rs`, `values.rs`, `tests.rs`. Parent 996 lines: `Edit`, `show`, attributes, tags, shared row helpers and `transparency_slider` (kept here because the UI Designer's crate move takes it) |
| R2 | `editor/src/app/settings.rs` | 2,208 | The window, the 400-line table of rows universal search can find, the Graphics page (440 lines), the look pages and their theme preview, tests inline | Split: `settings/search.rs`, `graphics.rs`, `appearance.rs`, `tests.rs`. Parent 624 lines: the window, the page list, Shortcuts |
| R3 | `editor/src/material_editor.rs` | 1,864 | The tab and its state, plus 870 lines of value rows (Shading, Toon, Outline, Water, maps) | Split: `material_editor/values.rs` beside the existing `graph.rs`. Parent 991 lines |
| R4 | `editor/src/asset_browser.rs` | 2,004 | State, scanning and file operations, and 1,040 lines of drawing in one `impl` | Split: drawing into `asset_browser/panel.rs` (a second `impl AssetBrowser`). Parent 960 lines |
| R5 | `script/src/lib.rs` | 2,375 | One 1,820-line `impl ScriptRuntime` covering chat, players and networking, the console, every frame step and event, and the debugger | Split by what the methods serve into `runtime/{chat,players,console,frame,debugging}.rs`, each another `impl ScriptRuntime`. `lib.rs` 1,117 lines: module list, re-exports, the struct, construction, starting and reloading scripts, camera, graphics and output |
| R6 | `scene/src/draw_list.rs` | 2,287 | Lighting settings and light placement, materials and textures, item building, and 680 lines of tests in one file | Split: `draw_list/lights.rs`, `materials.rs`, `tests.rs`. Parent 880 lines: building the list and its items |
| R7 | `player/src/main.rs` | 2,142 | `main`, the `App`, the 300-line `HostIo` impl the frame schedule calls, and the window event handler | Split: `host_io.rs` and `window.rs` (the `ApplicationHandler` and its helpers). `main.rs` 1,527 lines |

**Seen, not changed:**

| # | File | Why left |
|---|---|---|
| R8 | `script/src/definitions/docs.rs`, `tables.rs` (5,468, 3,983) | Data: the API's text and Luau types, one entry per line. Splitting a table helps nobody |
| R9 | `core/src/class.rs` (3,320) | The class table: one cohesive list, already ordered by class |
| R10 | `render/src/renderer/scene.rs` (2,890) | Most of it is `ScenePipeline::draw`, one 1,020-line function that orders the passes. Cutting it by line would only hide the order; the real fix is the deferred render graph (L8, [Technical debt](../../../wiki/Roadmap/Technical-Debt.md)). Performance-sensitive, so not touched without a measured reason |
| R11 | `blocks/src/from_lua.rs` (2,663), `blocks-runtime/src/lib.rs` (2,174) | One job each (Lua to blocks, blocks to Lua); a split would cut the shared match arms apart |
| R12 | `editor/src/block_editor.rs` (2,252), `viewport.rs` (1,823), `explorer.rs` (1,680), `app.rs` (1,909) | Already parent-plus-folder; what's left in the parent is the type and its frame. `app.rs`'s `EditorApp` has 134 fields: shrinking that is G11's `EditorContext`, not a file split |
| R13 | `editor/src/model_studio.rs`, `gizmo.rs`, `snapping`, `shortcuts.rs`, `animation/`, `ui_designer.rs`, `node_graph.rs`, `style_panel.rs` | Being moved into their own crates by G11 now; splitting them here would collide. Items the UI Designer move takes from Properties and the Assets panel (`Edit`, `transparency_slider`, `AssetKind`, `AssetPayload`, `TexturePick`) stay in the parent files |
| R14 | Test files (`blocks/src/tests.rs` 3,278, `player/src/tests.rs` 3,069, `app/mcp/tests.rs` 2,603, …) | Long but flat: one test per behavior, already beside the code they test |
| R15 | `editor-toolbox/src/lib.rs` (2,239) | Moved into its own crate by G11 last week; its owner thread splits it if needed |

## Duplication and coupling

- **Three graph canvases.** `node_graph.rs` is the shared editor (behavior trees, dialogues, material graphs and UI event graphs use it), but block scripts' Graph view (`block_editor/graph.rs`) and the Animation tab's state machine (`animation/graph.rs`) each draw their own pan, zoom and wires. Merging them is the design system's V24 "One graph canvas" ([design-system.md](design-system.md)), an architecture change with its own plan; not done here.
- **`with_causes`** (an error and its causes on one line) is written four times, in `assets`, `render`, `project` (`causes`) and the editor's Material tab. Each is eight lines in a different crate with no shared dependency to put it in; a shared crate for it would add coupling, not remove it. Left.
- **Crate `CLAUDE.md` files.** Root `CLAUDE.md` says to keep them under 10 lines. `editor/CLAUDE.md` and `script/CLAUDE.md` are 8 lines but each line is a paragraph (the editor's is ~10,000 characters, a file map). Nearly every editor PR edits it, which is where most merge conflicts come from (two bullets appended at once). Moving the file map to `docs/editor/` and the script one to `docs/scripting.md` would make both the steering files and the merges smaller; listed as a follow-up, since every open thread edits these files.

## What this changes for readers

- Each new file opens with a `//!` comment that says what it holds, so the file list reads as a table of contents.
- [crates.md](../../architecture/crates.md#where-the-big-files-split) lists the new parent/child rows, and the crate `CLAUDE.md` lines that name moved code point to the new files.
- Public paths are unchanged: moved public items are re-exported from the parent (`pub use`), so callers and other crates don't change.

## Verification

Behavior can't change from a verbatim move, and the compiler checks every path. Each PR runs the affected crates' full test suites before and after (results in the PR), plus `cargo clippy` on them. Known cloud-only failures (no audio device, GPU-less tests) are listed as such.
