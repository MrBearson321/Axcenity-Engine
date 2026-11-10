# `cook`

- Cooks a project's textures (roadmap L2): every image under `assets/` gets its mip levels made and BC7-encoded (`intel_tex_2`) into `.axcenity/imported/textures/`. The file format, freshness check and reading are in `assets` (`cooked.rs`); behavior in `docs/rendering/textures.md`.
- Editor only: `player`, `server` and `testing` must never depend on this crate (it links Intel's prebuilt ISPC kernels). `build.rs` links the C++ runtime they need outside MSVC.
- Mips here must match the renderer's (`render/src/renderer/gpu.rs`, `half_size`): the same 2×2 box average.
