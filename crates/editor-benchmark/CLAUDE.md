# `editor-benchmark`

- Debug > Graphics Benchmark: in-memory workloads, its own renderer, the report and the recommended editor graphics settings, and the window. Open `docs/rendering/settings.md` ("Editor benchmark").
- Never touches the open project or the game's `graphics.json`; Apply happens in the editor (`app/graphics_benchmark.rs`, which imports this crate as `graphics_benchmark`).
- `testing` (GPU device, quick run) is behind the `test-support` feature, for the editor's tests; turn it on from `[dev-dependencies]` only.
