# Graphics settings

The machine's graphics choices: presets, overrides, what the GPU can do, and who changes them. How each setting draws is in the file for its feature (listed at the end).

## Look and cost

Two kinds of setting, never mixed:

- **Look** belongs to the world: Lighting (`ToneMapping`, `Exposure`, bloom, `EnvironmentIntensity`, `SkyImage`, `AmbientOcclusionIntensity`, shadows) and the lights. It's saved in `world.json`, and Lua, blocks and the Properties panel change it.
- **Cost** belongs to the machine: `render::GraphicsSettings` (from `render-types`) (`render-types/src/graphics.rs`). It's saved per machine (the editor's settings, the player's `graphics.json`), never in a project.

Where a Lighting value is also a cost, the world's value is a request and the settings are a ceiling: shadows draw only if `ShadowsEnabled` and the shadow quality allow them, out to the smaller of the two distances (`lighting::shadows_drawn`), and post-processing runs only if `PostProcessingEnabled` and the settings allow it (`lighting::post_drawn`). No Lighting property changes meaning.

## Presets and overrides

`GraphicsSettings` is a `quality` preset plus an optional override for each value. An override replaces only its own value, and changing the preset keeps the overrides (in the editor and from Lua alike).

| Setting | Low | Medium | High | Ultra |
|---|---|---|---|---|
| Shadow quality (sun map; spot maps; point maps, six faces each) | Low: 1024² × 2; none; none | Medium: 2048² × 2; 2 at 512²; 1 at 512² | High: 2048² × 3; 4 at 1024²; 2 at 512² | Ultra: 4096² × 4; 8 at 2048²; 4 at 1024² |
| Shadow distance ceiling | 75 studs | 100 | 150 | 300 |
| Post-processing | off | on | on | on |
| Render scale (50% to 200%) | 75% | 100% | 100% | 100% |
| Texture quality | Quarter | Half | Full | Full |
| Texture memory budget (16 MB to 64 GB) | 256 MB | 512 MB | 1024 MB | 2048 MB |
| Anti-aliasing (TAA is an override only) | Off | FXAA | MSAA 4× | MSAA 4× |
| Ambient occlusion | Off | Low (8 samples) | High (16) | High |
| Reflection quality (environment maps, reflection probes) | Low: the gradient, no sky image or probes | Medium: 64², 32 samples | High: 128², 64 | Ultra: 256², 128 |
| Contact shadows | off | off | on | on |
| Real-time GI (light probe volumes traced again) | off | off | off | off |
| Volumetric fog (sun shafts and lamp light in an Atmosphere's fog) | off | off | off | on |
| Cloud quality (steps through the layer; steps toward the sun) | Low: a flat sheet | Medium: 16; none | High: 32 with detail; 3 | Ultra: 64 with detail; 6 |
| Depth of field (samples per pixel) | Off | Low (16) | High (32) | High |
| Vignette, film grain, chromatic aberration, sharpen (each its own switch) | off | on | on | on |
| Motion blur (samples per pixel) | Off | Low (8) | High (16) | High |
| Hardware ray tracing (the GPU opened with ray queries at the next start, [ray-tracing.md](ray-tracing.md)) | off | off | off | off |
| Ray tracing distance (50 to 5,000 studs; used only on a GPU that can ray trace, by a ray traced effect) | 150 studs | 250 | 400 | 800 |
| Ray traced shadows (rays a pixel for soft shadows; only on a GPU that can ray trace, [ray-tracing.md](ray-tracing.md)) | Off | Off | Off | Off |
| Ray traced ambient occlusion (2, 4 or 8 rays a pixel at half size; only on a GPU that can ray trace) | Off | Off | Off | Off |
| Projected decals (follow the preset; no override) | none | 128 within 150 studs | 512 within 300 | 1024 within 600 |

High is the look worlds were made with. `automatic` (Lua's `Enum.GraphicsQuality.Automatic`) means the quality follows the machine: `GraphicsSettings::automatic(&Capabilities)` is High, or Low on a software adapter (`DeviceType::Cpu`), with no overrides. It's what a machine starts with (`default_for`), saved as `"automatic": true`, and picked again for the machine on each start (`player::graphics::fit_machine`).

## Capabilities

`render::Capabilities` is read from the device once (`Capabilities::read`): the largest 2D texture and array, timestamp queries, whether it's a software adapter, and the sample counts both scene formats support. `GraphicsSettings::effective(&Capabilities)` gives the `EffectiveGraphics` the renderer uses: a shadow quality whose maps don't fit drops to the best that does (a point light's six faces are six layers), an MSAA count the GPU lacks drops to the next mode down (4× → 2× → FXAA), a reflection cubemap larger than the GPU's textures drops to one that fits (or Low), whether the device can trace rays (`Capabilities::ray_queries`, [ray-tracing.md](ray-tracing.md)), and real-time GI is off on a GPU without compute shaders (`Capabilities::compute_shaders`: no compute workgroups or storage buffers in its limits, as on WebGL 2), each with a note saying why. It never raises a value. The render scale is held to the largest texture the GPU can make (`scene::scene_size`), and textures leave out levels wider than it (`gpu::create_texture`).

`Renderer::set_graphics` and `TextureRenderer::set_graphics` apply settings from the next frame; settings equal to the last ones do nothing. Every frame target exists only while a setting uses it, and a change frees what's no longer used. `TextureRenderer::limit_capabilities` draws as if the GPU could do less (each value the lower of the two), for checking fallbacks on a GPU that can do more.

## Who changes them

- **Editor**: Settings > Graphics (`editor/src/app/settings/graphics.rs`) shows the preset, each override and what's in effect, with the notes. It's saved with the editor's settings, and the viewport uses it at once.
- **Editor benchmark**: Debug > **Graphics Benchmark…** (`editor-benchmark/src/lib.rs`, any build) measures how this computer draws the viewport and recommends the *editor's* settings only. It makes its own `TextureRenderer` on the editor's device and draws worlds of its own built in memory (`editor-benchmark/src/workloads.rs`; the open project, its Lighting and the game's `graphics.json` are never read or written), at the viewport's size: a test town (about 2,000 parts of every shape, glass, neon and metal, a sun, 16 lamps with 6 casting shadows, an Atmosphere and Clouds) at each preset, Low to Ultra, shown as Performance, Balanced, High Quality and Maximum Quality, then Geometry (20,000 parts), Materials (every material in 8 variants) and Lights (80 lamps, 24 with shadows) at the recommended settings. The camera circles each world (`ORBIT_FRAMES`), the same views for every preset. Per frame it times the draw list and recording on the CPU and the GPU's own timestamps per pass (`RenderStats`), waiting for the GPU after each frame; a frame is the longer of the two (the whole wait without timestamps). Each measurement keeps the median, 95th and 99th percentile and worst frame, frames over twice the median (hitches), the first frame after the switch (pipelines, shaders, targets), the GPU memory the renderer holds and the passes' medians; making the renderer is timed as setup. A preset over three times the budget skips the ones above it. The rules (`editor-benchmark/src/recommend.rs`): the best preset whose 95th-percentile frame is within 11.1 ms (two-thirds of a 60 fps frame); when the next preset up misses, it is measured again with the setting whose pass grew most set back (shadows, ambient occlusion, volumetric fog, anti-aliasing or clouds) and recommended if that fits; when even Low misses, a GPU-bound machine gets a lower render scale (by the square root of the time over, 50% at least) and a CPU-bound one is told settings won't help much; a software adapter gets Low. The texture budget becomes a quarter of the card's memory, counted in whole GB as its box says (a power of two, 256 MB to 8 GB), where it's lower than the preset's, or higher at Ultra. The card's memory comes from the display driver's registry entry on Windows and amdgpu's `mem_info_vram_total` on Linux (`editor-benchmark/src/video_memory.rs`, no unsafe code), otherwise it's not reported. The window lists the reasons, a Now/Recommended table of every setting that would change, **Apply Recommended** (Undo puts the old settings back; it keeps what the benchmark doesn't weigh as the person set it: real-time GI, the lens effects and motion blur, and a texture budget of at most half the card's memory, `recommend::apply_to`), **Keep Current Settings** and **Run Again**; nothing changes without Apply, and starting is refused during Play. The last report is saved with the editor's settings (`graphics benchmark` key, versioned) and shown when the window opens again, with a warning if it was measured on another graphics card.
- **Player**: `graphics.json` in the game's player data folder (`save::game_folder`, `player/src/graphics.rs`). The first start writes Automatic. A file that doesn't read as settings is warned about and replaced by the default. The notes go to the log.
- **LocalScripts**: `game:GetService("UserGameSettings")` (`script/src/user_settings.rs`) has `GraphicsQuality` (`Enum.GraphicsQuality`: Automatic, Low, Medium, High, Ultra) and the overrides `ShadowQuality` (`Enum.ShadowQuality`), `ShadowDistance`, `PostProcessing`, `RenderScale` (kept within 0.5 to 2), `TextureQuality` (`Enum.TextureQuality`), `AntiAliasing` (`Enum.AntiAliasing`: Off, FXAA, MSAA2, MSAA4, TAA), `AmbientOcclusion` (`Enum.AmbientOcclusion`), `ReflectionQuality` (`Enum.ReflectionQuality`: Low, Medium, High, Ultra), `CloudQuality` (`Enum.CloudQuality`: Low, Medium, High, Ultra), `DepthOfField` (`Enum.DepthOfField`: Off, Low, High), `MotionBlur` (`Enum.MotionBlur`: Off, Low, High), `RayTracedShadows` (`Enum.RayTracedShadows`: Off, Low, Medium, High; Off on a GPU that can't ray trace), `RayTracedAmbientOcclusion` (`Enum.RayTracedAmbientOcclusion`, the same choices), and `ContactShadows`, `RealtimeGI`, `VolumetricFog`, `Vignette`, `FilmGrain`, `ChromaticAberration` and `Sharpen` (true or false). Reading one gives the value chosen (the override, or the preset's), not what the GPU lowered it to, and setting an override to nil goes back to the preset's. The texture memory budget and the ray tracing distance aren't among them: they're the machine's to set, in the editor or `graphics.json` (`texture_budget`, in MB; `ray_tracing_distance`, in studs). A Script gets "UserGameSettings only works in a LocalScript…". Hosts hand the settings in with `ScriptRuntime::set_graphics(settings, capabilities)` before scripts start (the capabilities pick Automatic's quality) and take a change with `take_graphics_change`. The player then fits it to the machine, saves it to `graphics.json` and draws with it from that frame. The editor's Play starts from the editor's own settings, draws a script's change in the viewport (`Session::graphics`) and drops it at Stop. Blocks: "set graphics quality to", "set graphics *option* to" (a choice, or Preset for nil) and "set graphics *number* to" ([scripting/blocks.md](../scripting/blocks.md)). `examples/hello-graphics`' LocalScript steps through the presets on G.

## Tests

- `render-types/src/graphics.rs`: each preset's values, overrides, the `graphics.json` shape, and `effective` lowering and never raising.
- `player/src/graphics_tests.rs`: hello-graphics matches `reference-low.png`, `reference-medium.png`, `reference.png` (High, also checked by `every_example_renders`) and `reference-ultra.png`, at 480×270 within 6 per channel on average. After a deliberate change to the look, run `hello_graphics_matches_each_presets_reference` with `AXCENITY_WRITE_REFERENCES` set to rewrite Low, Medium and Ultra (High is rewritten as [pipeline.md](pipeline.md) says), and check them by eye. Also: lighting changes move every kind of material the same way, a LocalScript's changes show in the next frame and are saved, and a GPU limited to no MSAA and 1024² (then 64²) textures draws with fallbacks and notes.
- `editor-benchmark/src/recommend.rs` and `tests.rs`, `editor/src/app/graphics_benchmark_tests.rs`: each recommendation rule on made-up numbers, a whole quick run on the real device (every preset in order, the heavier tests at the recommendation, the report saved and read back), the changes table having a row for every setting, Apply keeping real-time GI, the lens effects and a budget that fits, the window in each state, a run leaving the project and the settings alone until Apply (and Undo), the report surviving a restart, and no start during Play.
- `render/src/renderer/tests.rs`: `switching_every_setting_and_dropping_renderers_leaves_no_resources_behind` switches every setting through every value, checks each target and map is alive only while used and that the caches come back to their starting counts, then drops the renderer and checks, with wgpu's report (`Instance::generate_report`), that the device holds no more textures, views, buffers, bind groups, pipelines, samplers or query sets than before.

## Each setting

- Shadow quality and distance, and contact shadows: [shadows.md](shadows.md); spot and point light shadows: [lights.md](lights.md).
- Render scale, texture quality and the texture memory budget: [shadows.md](shadows.md) (targets and textures) and [models.md](models.md) (textures).
- Anti-aliasing and ambient occlusion: [shadows.md](shadows.md), TAA and motion blur: [temporal.md](temporal.md); post-processing: [post.md](post.md).
- Reflection quality and real-time GI: [sky.md](sky.md).
- Volumetric fog: [fog.md](fog.md).
- Cloud quality: [clouds.md](clouds.md).
- Projected decals: [decals.md](decals.md).
- Ray tracing (the distance, and each ray traced effect): [ray-tracing.md](ray-tracing.md).
- Depth of field and the other lens effects: [post.md](post.md).
