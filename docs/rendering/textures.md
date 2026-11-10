# Cooked textures

How texture files get onto the GPU, and how cooking them ahead of time (roadmap L2, first part) makes that a file read and a quarter-size upload.

## Without a cooked copy

A texture file (PNG, JPEG, TGA or BMP: a SurfaceAppearance's map, a texture set's, or one a model uses) is decoded to RGBA8 by `assets` on a worker thread (`docs/project/assets.md`). The first frame that draws it then makes its mipmaps on the CPU, a 2×2 box average down to 1×1 (`render/src/renderer/gpu.rs`, `mip_levels`), and uploads every level: 4 bytes a pixel plus a third for the mips, about 85 MB for a 4096² map. A project with many 4K maps spent seconds of its first frame doing that (2.7 to 3 s of Render for Abandoned Farmhouse, `docs/benchmarks.md`).

## Cooking

`axcenity-cook` (editor only) cooks a texture file: decodes it, makes the same box-filtered levels down to 1×1, and BC7-encodes each one with Intel's ISPC compressor (`intel_tex_2`, "very fast" settings, opaque or alpha by whether any pixel is see-through). Levels smaller than a 4×4 block are padded by repeating their edge. The result is written to `.axcenity/imported/textures/<file name>-<hash of its project path>.axtex`. Like the rest of `.axcenity/`, it's this computer's: it isn't exported, backed up or put in version history, and deleting the folder only means cooking again.

- **When.** The editor cooks every image under `assets/` that has no current cooked copy on a thread of its own each time a project opens, with half the machine's cores, stopping between textures if another project opens (`editor/src/cooking.rs`). `axcenity-editor --cook <project folder>` does the same from the command line, printing each texture, and exits with 1 if any failed (for benchmarks and scripts). A texture is cooked once; editing the file makes its copy out of date, and the next open cooks it again.
- **What isn't cooked.** A texture whose width or height isn't a multiple of 4 (BC7 textures start with whole blocks) stays RGBA. So do images inside a model that importing hasn't copied out (a `.glb` dropped into `assets/` by hand), and generated textures (built-in surfaces, terrain weights, UI images drawn at their full size).
- **The file** (`assets/src/cooked.rs`): `AXTEX`, a format version, the source's length, modified time and a 64-bit hash of its bytes, then the format (BC7), size, whether it has alpha, the average of about 4,096 of its pixels, each level's length, and the levels, largest first. A cut or mismatched file is ignored.

## Loading a cooked texture

`Assets::texture` (and a model's textures, by the project path of their file) uses the cooked copy when it's current: the source file has the length it recorded and the same modified time, or, if only the time differs (a copied or unpacked project), the same bytes. A texture embedded in a model is matched by its embedded bytes against the file importing copied it out to. Anything else decodes the source as before, so a stale copy is never drawn.

A cooked `Image` holds its BC7 levels and decodes its source file only if something on the CPU asks for its pixels (`Image::rgba`: the Model Studio, thumbnails, color grading strips). Loading a model reads `Image::has_alpha` and `Image::sampled_average` instead, which a cooked image answers from what was recorded at cooking, so loading never decodes it.

## Drawing

`TextureSource::bc7_levels` hands the renderer the levels. A surface texture with them goes up as `Bc7RgbaUnorm` (read through a `Bc7RgbaUnormSrgb` view for colors, as RGBA8 is), with no mipmaps to make and a quarter of the bytes, when the device has `TEXTURE_COMPRESSION_BC`. The player's and the editor's devices ask for it whenever the GPU has it (every desktop GPU, and lavapipe). Otherwise, for UI images and terrain weights (drawn at full size from their pixels), and for levels that don't match the size, the renderer takes `rgba()` and the old path.

The texture quality and the GPU's largest size skip the largest levels as they do for RGBA, but the largest level kept must still be whole 4×4 blocks; the texture budget (`texture_budget.rs`) shrinks a BC7 texture down to its 4×4 level at most, copying whole blocks (`Texels::max_dropped`, `Texels::stored_size`). `GpuTexture::texels` says which a texture is; `RenderStats` counts BC7 textures at their real size (16 bytes a block).

## Not done yet

The rest of L2 is still open: sidecar files and stable asset IDs, cooked meshes and offline levels of detail, putting cooked textures into Build Game's archive (exported games still decode their textures, though the player reads cooked copies when run on a project folder), cooking right after an import or a file change rather than at the next open, and deleting cooked copies of files that are gone.
