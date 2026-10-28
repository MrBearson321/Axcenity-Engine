# Graphics settings

The machine's graphics choices: presets, overrides, what the GPU can do, and who changes them. How each setting draws is in the file for its feature (listed at the end).

## Look and cost

Two kinds of setting, never mixed:

- **Look** belongs to the world: Lighting (`ToneMapping`, `Exposure`, bloom, `EnvironmentIntensity`, `SkyImage`, `AmbientOcclusionIntensity`, shadows) and the lights. It's saved in `world.json`, and Lua, blocks and the Properties panel change it.
- **Cost** belongs to the machine: `render::GraphicsSettings` (`render/src/graphics.rs`). It's saved per machine (the editor's settings, the player's `graphics.json`), never in a project.

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
| Anti-aliasing | Off | FXAA | MSAA 4× | MSAA 4× |
| Ambient occlusion | Off | Low (8 samples) | High (16) | High |
| Reflection quality (environment maps, reflection probes) | Low: the gradient, no sky image or probes | Medium: 64², 32 samples | High: 128², 64 | Ultra: 256², 128 |
| Contact shadows | off | off | on | on |
| Real-time GI (light probe volumes traced again) | off | off | off | off |

High is the look worlds were made with. `automatic` (Lua's `Enum.GraphicsQuality.Automatic`) means the quality follows the machine: `GraphicsSettings::automatic(&Capabilities)` is High, or Low on a software adapter (`DeviceType::Cpu`), with no overrides. It's what a machine starts with (`default_for`), saved as `"automatic": true`, and picked again for the machine on each start (`player::graphics::fit_machine`).

## Capabilities

`render::Capabilities` is read from the device once (`Capabilities::read`): the largest 2D texture and array, timestamp queries, whether it's a software adapter, and the sample counts both scene formats support. `GraphicsSettings::effective(&Capabilities)` gives the `EffectiveGraphics` the renderer uses: a shadow quality whose maps don't fit drops to the best that does (a point light's six faces are six layers), an MSAA count the GPU lacks drops to the next mode down (4× → 2× → FXAA), a reflection cubemap larger than the GPU's textures drops to one that fits (or Low), and real-time GI is off on a GPU without compute shaders (`Capabilities::compute_shaders`: no compute workgroups or storage buffers in its limits, as on WebGL 2), each with a note saying why. It never raises a value. The render scale is held to the largest texture the GPU can make (`scene::scene_size`), and textures leave out levels wider than it (`gpu::create_texture`).

`Renderer::set_graphics` and `TextureRenderer::set_graphics` apply settings from the next frame; settings equal to the last ones do nothing. Every frame target exists only while a setting uses it, and a change frees what's no longer used. `TextureRenderer::limit_capabilities` draws as if the GPU could do less (each value the lower of the two), for checking fallbacks on a GPU that can do more.

## Who changes them

- **Editor**: Settings > Graphics (`editor/src/app/settings.rs`) shows the preset, each override and what's in effect, with the notes. It's saved with the editor's settings, and the viewport uses it at once.
- **Player**: `graphics.json` in the game's player data folder (`save::game_folder`, `player/src/graphics.rs`). The first start writes Automatic. A file that doesn't read as settings is warned about and replaced by the default. The notes go to the log.
- **LocalScripts**: `game:GetService("UserGameSettings")` (`script/src/user_settings.rs`) has `GraphicsQuality` (`Enum.GraphicsQuality`: Automatic, Low, Medium, High, Ultra) and the overrides `ShadowQuality` (`Enum.ShadowQuality`), `ShadowDistance`, `PostProcessing`, `RenderScale` (kept within 0.5 to 2), `TextureQuality` (`Enum.TextureQuality`), `AntiAliasing` (`Enum.AntiAliasing`: Off, FXAA, MSAA2, MSAA4), `AmbientOcclusion` (`Enum.AmbientOcclusion`), `ReflectionQuality` (`Enum.ReflectionQuality`: Low, Medium, High, Ultra), `ContactShadows` and `RealtimeGI` (true or false). Reading one gives the value chosen (the override, or the preset's), not what the GPU lowered it to, and setting an override to nil goes back to the preset's. The texture memory budget isn't among them: it's the machine's to set, in the editor or `graphics.json` (`texture_budget`, in MB). A Script gets "UserGameSettings only works in a LocalScript…". Hosts hand the settings in with `ScriptRuntime::set_graphics(settings, capabilities)` before scripts start (the capabilities pick Automatic's quality) and take a change with `take_graphics_change`. The player then fits it to the machine, saves it to `graphics.json` and draws with it from that frame. The editor's Play starts from the editor's own settings, draws a script's change in the viewport (`Session::graphics`) and drops it at Stop. Blocks: "set graphics quality to", "set graphics *option* to" (a choice, or Preset for nil) and "set graphics *number* to" ([scripting/blocks.md](../scripting/blocks.md)). `examples/hello-graphics`' LocalScript steps through the presets on G.

## Tests

- `render/src/graphics.rs`: each preset's values, overrides, the `graphics.json` shape, and `effective` lowering and never raising.
- `player/src/graphics_tests.rs`: hello-graphics matches `reference-low.png`, `reference-medium.png`, `reference.png` (High, also checked by `every_example_renders`) and `reference-ultra.png`, at 480×270 within 6 per channel on average. After a deliberate change to the look, run `hello_graphics_matches_each_presets_reference` with `AXCENITY_WRITE_REFERENCES` set to rewrite Low, Medium and Ultra (High is rewritten as [pipeline.md](pipeline.md) says), and check them by eye. Also: lighting changes move every kind of material the same way, a LocalScript's changes show in the next frame and are saved, and a GPU limited to no MSAA and 1024² (then 64²) textures draws with fallbacks and notes.
- `render/src/renderer/tests.rs`: `switching_every_setting_and_dropping_renderers_leaves_no_resources_behind` switches every setting through every value, checks each target and map is alive only while used and that the caches come back to their starting counts, then drops the renderer and checks, with wgpu's report (`Instance::generate_report`), that the device holds no more textures, views, buffers, bind groups, pipelines, samplers or query sets than before.

## Each setting

- Shadow quality and distance, and contact shadows: [shadows.md](shadows.md); spot and point light shadows: [lights.md](lights.md).
- Render scale, texture quality and the texture memory budget: [shadows.md](shadows.md) (targets and textures) and [models.md](models.md) (textures).
- Anti-aliasing and ambient occlusion: [shadows.md](shadows.md); post-processing: [post.md](post.md).
- Reflection quality and real-time GI: [sky.md](sky.md).
