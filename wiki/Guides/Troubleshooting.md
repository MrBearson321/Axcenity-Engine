---
title: Troubleshooting
type: guide
status: current
verified: 2026-11-10
docs: [docs/development.md]
tags: [guide, troubleshooting]
---

# Troubleshooting

Known problems with building, testing and running the engine, and their fixes.

| Symptom | Cause | Fix |
|---|---|---|
| `Blocking waiting for file lock` | rust-analyzer or another agent's build holds the target folder | rust-analyzer has its own target dir (`.vscode/settings.json`); otherwise wait, or use a separate worktree target |
| Machine swaps during a cold build | Up to ~2.4 GB per compiler process | `cargo build -j 4` or `CARGO_BUILD_JOBS=4` |
| `We timed out while waiting on the last successful submission to complete!` in GPU tests | Too many GPU tests sharing one device | nextest's `gpu` group limits to 8; run the failing test alone |
| `every_example_renders` reports an example "renders almost one color" | Known rare flake under GPU contention | Rerun; investigate only if it repeats |
| `luau_sources_stay_pinned` fails | `mlua` or `luau0-src` changed alone, or CRLF in `Cargo.lock` on Windows (fixed) | Raise both together |
| `definitions_file_is_up_to_date` fails | Script API changed | `cargo run -p axcenity-script --bin lua-definitions` |
| Workspace-hack check differs | Dependencies changed | `cargo hakari generate` |
| Player test can't find the GPU / blank frames on a headless Linux box | No adapter | Run GPU tests on a machine with a GPU; the server and test runner need none |
| Editor package tests fail in a fresh worktree | Ground108 4K textures aren't in git | Known; they pass in the main checkout |
| Stale artifacts across worktrees sharing `CARGO_TARGET_DIR` | Shared target folder | `cargo clean -p <crate>` |
| `git push` says "Repository not found" | The active `gh` account isn't `MrBearson321` | Check `gh auth status`; switching is the owner's call |
| Editor window brightens briefly (OLED or VA monitor with VRR) | VRR following uneven frame times | FreeSync off, release build, or Settings > Accessibility "Hold the refresh rate steady" ([Hardware notes](../Performance/Hardware-Notes.md)) |
| Huge project is slow in the editor | Debug build, Ultra, real-time GI | Measure in release with `bench_project` first ([Benchmarking](../Performance/Benchmarking.md)) |
| A model shows as a grey box | Missing or broken file | Asset Check (File > Check Assets) lists it with fixes |
| A model draws nothing for a moment | Still loading (by design since M5) | Wait; `Assets::progress` shows loading |
| Startup slow on a project with embedded 4K maps | Cooking hasn't run | Open it in the editor once, or `axcenity-editor --cook <project>` |
