# Motion vectors, TAA and motion blur

Graphics Phase 35. The passes that keep last frame's image: TAA, the fifth anti-aliasing mode, and a `MotionBlurEffect`'s streaks. Both read the motion vectors, which say how far each pixel moved across the screen since last frame. Code: `render/src/renderer/temporal.rs` and `temporal.wgsl`, plus `vs_motion` and `fs_motion` in shader.wgsl.

## Motion vectors

They exist only while TAA is on or motion blur is drawn. Then the depth and normal prepass is kept alive (as for ambient occlusion, [shadows.md](shadows.md)), and `FrameTargets::temporal` (`TemporalTargets`) holds two `Rg16Float` motion targets at the scene's size. Each frame draws into one, by `History::parity`, and TAA reads the other as last frame's. A motion vector is this frame's place on screen minus last frame's, in texture coordinates (0 to 1 across the scene, down the screen), both without TAA's jitter.

Right after the prepass:

1. `fs_camera_motion` works out the camera's motion at every pixel. It finds the prepass's surface there in the world (with this frame's view as drawn, jitter and all), or the far plane where nothing solid was drawn (the sky), and projects it with this frame's and last frame's views.
2. Items that moved draw their own motion over it (`fs_motion`), where the prepass's depth shows them (`LessEqual`, read-only, so hidden ones leave no motion). Both passes place corners the same way, and `@invariant` on the clip position keeps their depths equal. An item moved when it has a `DrawItem::id` (not 0), is solid (drawn in the prepass, so not see-through, water or terrain), was drawn last frame, and its transform changed or it's posed. A posed item's corners are moved by last frame's joints (`previous_joints`, laid out as this frame's in group 3) as well as this frame's, so a posed mesh's motion comes from its animation too. `History` keeps last frame's camera, and each item's transform and joints by ID (`History::finish`). Its own instance buffer (`MotionInstance`) holds each moved item's two transforms, so the scene's instance layout isn't touched.

An item seen for the first time, or with ID 0, moves only with the camera. A swaying plant's motion leaves out the sway. See-through items and sprites aren't in the prepass, so they take the motion of what's behind them.

## TAA

`AntiAliasing::Taa` (`"TAA"` in `Enum.AntiAliasing`, the blocks and the editor). It draws with one sample per pixel, so every GPU can do it and `effective` never lowers to it or from it. No preset picks it.

- **Jitter.** The scene's view is moved by part of a pixel each frame (`History::jitter`, `jittered`): the 8 places of a Halton (2, 3) sequence, each within half a pixel of the middle. The scene, prepass, sky and sprites all draw with it; culling, light tiles and shadows don't.
- **History.** `TaaTargets` holds two HDR textures at the scene's size: each frame `fs_taa` blends the scene into one, reading last frame's from the other where each pixel was (its motion back from here, taken from the nearest surface in the 3×3 around it so a moving part's edge moves with it), filtered. This frame counts `TAA_SHARE` (0.1); bright pixels count for less in the blend, so one bright frame doesn't flicker. The result is copied over the scene's HDR texture, so depth of field, bloom and the composite pass read it as before.
- **No ghosts.** Last frame's color is held within the colors of the 3×3 around the pixel now (in YCoCg), so it can't show what's no longer there. Where the pixel's motion differs from last frame's at the place it came from by more than half a pixel (something moved off it and uncovered what's behind), the history counts less, and from two pixels of difference not at all: the pixel starts again from this frame.
- **Starting over.** The first frame after the targets are made (a new size, render scale or anti-aliasing), and the first after a frame without TAA, has no history and shows the frame as drawn (`History::valid`).
- **Limits.** Shapes thinner than about a pixel can still shimmer: what the current frame misses isn't in the 3×3 around a pixel, so its history is held to the background. A shadow moving over a still floor, or a sprite with no motion of its own, is held to the colors around it, so it doesn't trail. Reflection and light probe bakes draw without TAA (`ProbeCamera::set_graphics`).

### Upscaling inside TAA

Below 100% render scale with TAA on (roadmap G9 stage 2), TAA upscales the picture itself instead of the spatial upscale ([post.md](post.md)). It's automatic: no setting of its own.

- **History at the output's size.** `TaaTargets`' two history textures, and an output-sized HDR target (`TaaTargets::upscaled`) the result is copied into, are at the output's size while the scene is drawn smaller. Bloom and the composite pass read that target in the scene's place (`TemporalTargets::upscaled_view`), so they run at the output's size, and the post pass skips EASU: only the RCAS sharpen runs after it.
- **More jitter places.** Each output pixel gets fewer of the scene's, so the jitter steps through more places of the Halton sequence before it repeats (`jitter_phases`): 8 × the area ratio, as FSR 2 does, held to 8 to 64 (`MAX_JITTER_PHASES`). 18 at 67%, 32 at 50%.
- **Gathering samples.** `fs_taau` takes the 3×3 of the scene's pixels nearest each output pixel, each weighted by how far its jittered center is from the output pixel's (a Gaussian, `sample_weight`). The same samples make the YCoCg box that holds the history, the nearest surface gives the motion, and disocclusion works as at full size. Last frame's color is read with a 5-tap Catmull-Rom filter (`history_at`), so the history stays sharp over many frames. This frame's share is larger where a scene sample falls close to the pixel and smaller where none does.
- **Order.** At full size, TAA runs before motion blur and depth of field. When TAA upscales, motion blur and depth of field run first on the scene at its size, and TAA reads depth of field's result when it's on (`SceneViews::taa_source`), because their passes and the prepass they read are at the scene's size.
- **Limits.** Textures aren't sampled with a mip bias at lower scales, so fine textures look as soft as the scene's size, not the output's. Thin shapes can shimmer more than at full size, for the reason above. The first frames after a change of size or scale start from the frame as drawn, upscaled with no history.

## Motion blur

A `MotionBlurEffect` inside Lighting (`Enabled`, `Intensity` 0.5), read by `scene/src/lens.rs` into `LensEffects::motion_blur` like the other lens effects ([post.md](post.md)); the first enabled one in Lighting is used, and one anywhere else does nothing. `Intensity` is the share of a frame's motion that streaks: 1 is a shutter open the whole frame. A streak is at most 1/18 of the scene's height long (`MAX_MOTION_BLUR`, half of it each way). The graphics setting `motion_blur` (`MotionBlurQuality`: Off, Low 8 samples, High 16; Off at Low, Low at Medium, High above; `UserGameSettings.MotionBlur`, `Enum.MotionBlur`) lets a machine turn it off or down. Off, absent, or at `Intensity` 0, it costs nothing: no motion targets (unless TAA has them), no prepass for it, no passes.

It runs after TAA (before it while TAA upscales, above), before depth of field and bloom, as McGuire's reconstruction filter (2012) does:

1. `fs_tile_max` finds the longest streak in each tile of the screen (tiles as wide as the longest half-streak, `blur_tile`).
2. `fs_motion_blur` streaks each pixel along the longest streak in the 3×3 tiles around it, in evenly spaced samples both ways, turned per pixel by interleaved noise. A sample counts if it's in front and its own streak reaches back to this pixel, or behind and this pixel's streak reaches it, or both move alike. So a moving part smears over the still background behind it, and a still part stays sharp. Where nothing nearby moves more than half a pixel, the pixel is left exactly as it was.

The result goes into `BlurTargets`' HDR target and is copied over the scene's. Motion is relative to the camera: a part the camera follows stays sharp while the world streaks past it.

## The render graph

The Audit in `plans/axcenity-engine-graphics.md` reopened the question here. It's still not built. The two features add a fixed stretch to the frame: the motion passes after the prepass, then TAA and motion blur before depth of field (or motion blur, depth of field and TAA while TAA upscales), each decided once per frame by what `FrameTargets` holds. What's kept between frames is in one place (`History`, and the paired targets it picks by parity), made and freed with the other frame targets by use, so the frame stays one readable function with a fixed order.

## Tests

`render` `renderer/tests/temporal.rs`: `motion_vectors_hold_how_far_moved_parts_posed_meshes_and_the_camera_went`, `taa_smooths_edges_and_thin_geometry_over_frames` (a tilted block over the sky, and a glowing bar 1.2 pixels thick whose every column keeps its light), `taa_leaves_no_ghost_behind_a_moving_part` (a part crossing a wall for 12 frames leaves the wall as drawn without TAA), `motion_blur_streaks_only_what_moves_relative_to_the_camera` (with the camera still and then following the moving part) `upscaling_taa_leaves_no_ghost_behind_a_moving_part` (the same at half scale) and `motion_targets_exist_only_while_taa_or_motion_blur_use_them`. `renderer/tests/upscaling.rs`: `taa_upscales_half_scale_closer_to_full_size_than_the_spatial_upscale`, `taa_upscales_with_output_sized_history_and_no_spatial_upscale` and `flat_colors_come_through_taa_upscaling_unchanged`. `renderer::temporal::tests` (the jitter, and `upscaling_steps_through_more_jitter_places_as_pixels_grow`), `graphics::tests::motion_blur_has_its_own_setting_and_taa_is_never_lowered`, the setting-switching resource test, `scene` `lens::tests`, and `script` `user_settings_tests::taa_and_motion_blur_are_settings_for_local_scripts`.
