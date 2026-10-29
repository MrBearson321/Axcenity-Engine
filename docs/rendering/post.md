# Post-processing

HDR, bloom, tone mapping, color grading and lens effects.

- The scene draws into an `Rgba16Float` texture. A bloom chain (prefilter above `BloomThreshold`, downsample, additive upsample) feeds a composite pass that adds bloom, applies `Exposure` and tone mapping into the real target (or, with FXAA on, into an LDR target that FXAA then smooths into the real one), then the UI draws on top. With MSAA on, the scene pass draws multisampled and resolves into the HDR texture before bloom. Anti-aliasing is a graphics setting: [shadows.md](shadows.md). The machine's post-processing setting is a ceiling on `PostProcessingEnabled` (off on the Low preset; [settings.md](settings.md)), while `Exposure` and `ToneMapping` are the world's look, and no setting changes them.
- Lighting properties `PostProcessingEnabled`, `Exposure`, `ToneMapping`, `BloomEnabled`, `BloomIntensity` and `BloomThreshold` configure it. Off means a plain clamp with no bloom, whatever the curve.
- `ToneMapping` (`Enum.ToneMapping`, `render::ToneMapping`) picks the curve, after exposure:
  - `Classic`: each channel under 0.6 unchanged, brighter ones roll off toward 1 (it shifts the hue of bright colors). The default, so a world saved without the property looks as it did.
  - `Neutral`: Khronos PBR Neutral. Takes off the 4% a non-metal reflects (less in the darkest colors), so a lit part shows about its picked sRGB color, then compresses highlights without shifting hue. The editor's templates (`project::Template`) write it into new worlds.
  - `Filmic`: Stephen Hill's ACES fit, after scaling light by 1/0.6 so a mid-grey stays about as bright. More contrast; very bright colors wash toward white.
  - `render/src/tone_mapping.rs` is a CPU copy of the curves (`ToneMapping::apply`) that must match `post.wgsl`; a GPU test checks they agree. Scripts set it with `game.Lighting.ToneMapping = Enum.ToneMapping.Filmic` (or the name), and blocks with "set tone mapping to …".
- `scene` draws Neon parts 2.5× brighter than their color when bloom is on, so they pass the threshold and glow.

## Color grading

Graphics Phase 32. A `ColorCorrectionEffect` inside Lighting changes the finished picture's colors. The composite pass (`fs_composite` in `post.wgsl`) applies it after tone mapping and the clamp, on sRGB-encoded values (as the screen shows them), then turns them linear again for the sRGB target. With FXAA on, FXAA smooths the graded picture. It is the world's look. No graphics setting changes it, and it applies with post-processing off too (it costs one full-screen pass's arithmetic, not a target).

| Property | Default | What it does, in order |
|---|---|---|
| `Enabled` | true | Off, or with no effect in Lighting, nothing changes and nothing is bound. |
| `Brightness` | 0 | Added to each channel (-1 black, 1 white). |
| `Contrast` | 0 | `(c − 0.5) × (1 + Contrast) + 0.5`; -1 is flat grey. |
| `Saturation` | 0 | Mixed away from the Rec. 709 grey by `1 + Saturation`; -1 is black and white. |
| `TintColor` | white | Multiplies each color, then the result is clamped to 0–1. |
| `LookupTable` | "" | A `.cube` file or a strip image the color is looked up in. |
| `Posterize` | 0 | 2 or more cuts each channel into that many even levels (`min(floor(c × n), n − 1) / (n − 1)`). |
| `Palette` | "" | An image; each pixel becomes the nearest of its colors (straight distance in sRGB). |

Several enabled effects in Lighting add up: brightness, contrast and saturation add, and tints multiply. The first in Explorer order with a lookup table, a `Posterize` level or a palette gives that one (`scene/src/color_grading.rs`, into `DrawList::grading`). An effect anywhere else does nothing. `workspace.CurrentCamera` isn't an Instance, so effects can't go inside it; a `GraphicsVolume` gives a place its own look, and the effects inside one replace Lighting's while the camera is there ([volumes.md](volumes.md)).

**Files** (`assets/src/color_grading.rs`, read by `Assets::color_lookup` and `Assets::palette`, cached, reloaded when changed, and each failure reported once while colors stay as they are). The editor's pickers copy a file from outside the project into `assets/grading`.

- `.cube` (Adobe/Resolve): `LUT_3D_SIZE N` (2 to 128), then N³ lines of `r g b` with red changing fastest. `TITLE`, `#` comments and other keywords are skipped. `DOMAIN_MIN`/`DOMAIN_MAX` must be 0 and 1. 1D tables are refused.
- A strip image: N squares of N × N pixels side by side (256 × 16, 1024 × 32). Square `b` is blue `b/(N−1)`. In it red grows to the right and green downward. Its values are sRGB as stored.
- A palette: any PNG, JPEG, TGA or BMP image. Its different opaque colors, row by row, are the palette, up to 64 (`MAX_PALETTE_COLORS`). With more, a warning says only the first 64 are used.

**Drawing** (`renderer/grading.rs`). The composite pipelines have a second bind group: the lookup table as an `Rgba32Float` 3D texture and `GradingUniform` (matching `Grading` in `post.wgsl`: values, tint and on flag, counts, 64 palette colors). The shader reads the table's eight nearest entries with `textureLoad` and blends them itself, so a neutral table gives back exactly what it got and no float filtering is needed. The table's texture is made when its key changes and dropped when none is used (a 1×1×1 placeholder stays bound). A table with the wrong number of entries, or bigger than the GPU's 3D limit, is left out.

`examples/hello-grading` grades a colorful scene with a warm `.cube` table. Its LocalScript steps through looks with G: warm, cold night (tint), black and white, posterized, a retro palette, and none.

Tests: `render` `renderer/tests/color_grading.rs` (`each_control_changes_the_frame_as_it_says`; `a_neutral_lookup_table_changes_nothing`, with tables of 2, 17 and 33 and with Off, FXAA and MSAA; `a_lookup_tables_texture_lives_only_while_one_is_used`). `assets` `color_grading::tests` (`.cube` parsing and errors, strips, palettes). `scene` `color_grading::tests` (effects combine, and files load from the project).

## Lens effects

Graphics Phase 34. Five effect Instances inside Lighting, each off until one is added: `DepthOfFieldEffect`, `VignetteEffect`, `FilmGrainEffect`, `ChromaticAberrationEffect` and `SharpenEffect`. Of each kind, the first enabled one in Lighting (Explorer order) is used (`scene/src/lens.rs`, into `DrawList::lens`, a `render::LensEffects` of five options). They are the world's look. Each also has its own graphics setting ([settings.md](settings.md)), so a machine can turn any of them off, and Low turns them all off. `PostProcessingEnabled` doesn't affect them. An effect inside a `GraphicsVolume` does nothing ([volumes.md](volumes.md)).

| Effect | Properties (defaults) | What it does |
|---|---|---|
| `DepthOfFieldEffect` | `FocusDistance` 20, `InFocusRadius` 10, `NearIntensity` 0.75, `FarIntensity` 0.75 | Sharp from `FocusDistance − InFocusRadius` to `FocusDistance + InFocusRadius` studs. Nearer, the blur grows to `NearIntensity` at the camera; farther, to `FarIntensity` at twice the sharp range's far edge (the sky is fully far). Full blur is a radius of 1/72 of the scene's height (15 pixels at 1080). |
| `VignetteEffect` | `Intensity` 0.5, `Size` 0.5, `Color` black | From `Size` (0 the middle, 1 the corners) out to the corners, the picture turns toward `Color`, up to `Intensity`. Round on any screen. |
| `FilmGrainEffect` | `Intensity` 0.3, `Size` 1 | Noise of ±0.2 × `Intensity` in sRGB, a new pattern every frame, in grains `Size` pixels of a 1080-pixel-tall view across (never under one pixel). The average brightness stays. |
| `ChromaticAberrationEffect` | `Intensity` 0.5 | Red is read from farther out and blue from farther in, by 2% × `Intensity` of the distance from the middle (1% of the width at the side edges for 1): nothing in the middle, colored fringes toward the edges. |
| `SharpenEffect` | `Intensity` 0.5 | Each pixel moves away from the average of its four neighbors by 2 × `Intensity` of the difference: flat areas stay, edges steepen. Useful with a render scale below 100%. |

Each also has `Enabled` (true). Disabled, absent, or turned off in the settings, an effect costs nothing.

**Drawing.**

- **Depth of field** (`renderer/lens.rs`, `depth_of_field.wgsl`) is a pass of its own after the scene pass (and volumetric fog's light), before bloom. It needs the prepass's depth, so while it's drawn it keeps the prepass alive (like screen-space outlines). It draws into a blurred copy of the scene (`FrameTargets::depth_of_field`, an HDR target at the scene's size) that bloom and the composite pass read in the scene's place. Each pixel averages the pixels under a spiral of 16 (Low) or 32 (High) points on a disc as wide as its blur, each counting only as far as its own blur reaches back (`saturate(its blur − distance + 1)`). Color and depth come from the same whole pixel, so a sharp surface never smears into a blurred one beside it. A blurred surface in front of a sharp one doesn't spill over it either: near blur stops at its own silhouette. Depth of field that blurs nothing (both intensities 0) makes no target. See-through items aren't in the prepass, so they blur by what's behind them.
- **The other four** are in the composite pass (`fs_composite` in `post.wgsl`), from `PostUniform`'s `lens`, `vignette` and `vignette_color` (`lens::add_lens`; 0 is off and the shader skips it). Chromatic aberration and sharpening change how the scene (or its blurred copy) is read, before bloom and exposure. Sharpening is judged on the unparted scene, so fringes aren't sharpened. The vignette and grain are applied after tone mapping, on sRGB-encoded values, before color grading. Probe bakes draw without lens effects.

`examples/hello-lens` looks down two rows of pillars to a striped wall through all five. Its LocalScript steps through them one at a time, all together and none, with L.

Tests: `render` `renderer/tests/lens.rs` (`a_vignette_darkens_or_tints_toward_the_corners`, `film_grain_adds_moving_noise_and_keeps_the_average`, `chromatic_aberration_parts_red_and_blue_toward_the_edges`, `sharpening_steepens_edges_and_leaves_flat_areas` and `depth_of_field_blurs_out_of_focus_surfaces_and_leaves_focused_ones_sharp`, each also unchanged with its setting off; `lens_effects_cost_nothing_while_absent_or_turned_off`, which counts the targets and their bytes), `graphics::tests::each_lens_effect_has_its_own_setting_by_preset_and_override`, and the setting-switching resource test. `scene` `lens::tests`. `script` `user_settings_tests::each_lens_effect_has_its_own_setting_for_local_scripts`. `editor` `app::tests::lens_effects_go_into_lighting_with_their_own_properties_section`.
