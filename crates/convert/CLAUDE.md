# `convert`

- Editor-only: turns model formats the runtime doesn't read into a `.glb` before the Import dialog opens (ADR-0027, `docs/editor/import.md`). USD so far (`usd.rs`, `openusd` + `openusd-schemas`); textures and `.usdz` archives: `textures.rs`; the GLB writer: `glb.rs`.
- No engine-crate dependencies (`assets` only in tests, to load what it writes). The player and server must never link it (`scripts/check-platform.*`).
- Never panic on a bad file (`convert` catches panics) and never stop for something left out: report it in `Converted::warnings`, one line each.
- Output is glTF space: Y up, meters, right-handed, counter-clockwise front faces; each drawn prim is a node named as in the file, so the Objects tab shows the file's tree.
