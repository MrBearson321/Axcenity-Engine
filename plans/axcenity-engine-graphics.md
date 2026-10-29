# Plan: Axcenity Engine, graphics

> Source: the developer's graphics engine brief of 2026-10-26 (Phases 0 to 8 there; Milestones 1 to 8 here). Audited against `main` at commit `ceda19b`. The phases run in order inside a milestone. Each milestone's acceptance criteria must hold before the next starts, unless a phase names a dependency on a later one. Read the phase you're on, not the whole file.
>
> The brief's Phase 0 (the audit) is done: it is the **Audit** section below. Milestone 1 is the brief's Phase 1, the rendering foundation, cut into small phases that each build, test and ship on their own.

## Audit (brief Phase 0)

What the renderer is today, and what that means for the brief. Source and tests were read, not only the docs.

**Backend.** `wgpu` 25 with WGSL shaders, `glam`, and `glyphon` for UI text (`crates/render`, about 10k lines). DirectX 12 or Vulkan on Windows, Metal on macOS, Vulkan on Linux. The editor shares eframe's device through `TextureRenderer`. The player owns its window surface through `Renderer`. Both draw through the private `ScenePipeline` (`renderer/scene.rs`), so they draw the same. **Keep the backend.**

**Scene to frame.** The frame loop (`docs/architecture/frame-loop.md`, step 6) calls `scene::build_draw_list*`, which turns Instances into a `render::DrawList`: items, materials, sun, ambient, sky, shadows, lights, post, sprites and UI. `ScenePipeline::draw` then:

1. plans lights into 32 px screen tiles, writes the frame and cascade uniforms, and uploads new meshes, textures and materials (keyed caches, freed after 600 undrawn frames);
2. sorts items into instanced batches: solid items by mesh and material, then see-through items far to near (`plan_draws`);
3. draws three shadow cascades (2048², one layer each of a depth array);
4. draws the HDR scene pass (`Rgba16Float`): the sky triangle, the batches (scene or terrain pipeline), then sprites;
5. runs bloom (prefilter, downsample, upsample), then composites with exposure and the tone-mapping knee curve into the sRGB target;
6. draws the turned-text sheet, then the UI overlay.

The passes are hand-ordered in one function. There are six of them, and only bloom reads another pass's output.

**Reusable as it is.**

- **PBR.** GGX with Smith-Schlick and Schlick Fresnel (`lighting.wgsl`). Base color, metalness, roughness, emissive, opacity, alpha cutoff (MASK), BLEND transparency, and normal, metalness, roughness, occlusion, emission, opacity and specular maps (`TextureMaps`, channel picks and inversion). glTF and FBX map to it, as do Part properties (Color, Material preset, Metalness, Roughness, Transparency, EmissiveColor, Emission) and SurfaceAppearance.
- **Color.** Draw-list colors are sRGB and turned linear in the renderer. Textures are uploaded once with both an sRGB view (color maps) and a plain view (data maps). Shading is linear and HDR. The output target is sRGB.
- **Lights.** One sun, plus up to 1024 point and spot lights in tiles of up to 64. The procedural sky gives environment lighting (analytic, split-sum). Emissive light blooms.
- **Shadows.** The sun casts PCSS soft shadows over three cascades, snapped to texels, with slope bias and a fade at the edge. `ShadowsEnabled`, `ShadowDistance` and `ShadowSoftness` are Lighting properties. The shader already reads the cascade count from a uniform.
- **Instancing.** Per-item transform, normal matrix (the inverse transpose, so non-uniform `Size` lights correctly), color and surface values are per instance. Parts that differ only in color share a batch.
- **Resources.** GPU copies are keyed by `MeshSource::key` and `TextureSource::key`. Posed meshes are rewritten in place. Mipmaps are made on the CPU, and levels too big for the GPU are left out. A broken mesh or texture is logged once and skipped.
- **Diagnostics.** `RenderStats` (draw calls, triangles, GPU time where `TIMESTAMP_QUERY` exists) feeds the editor's Profiler.
- **Tests.** About 50 tests in `render/src/renderer/tests.rs`, many on a headless GPU device (`test_device`). The player test renders every example and, where an example has `reference.png`, fails if the frame differs by an average of 6 or more per channel. That is the visual regression harness: extend it, don't build a second one.

**Needs improvement.**

- Graphics choices that belong to the machine are on the world's `Lighting` service: `ShadowsEnabled`, and `ShadowDistance` as a cost. Nothing is scaled by hardware.
- The tone mapping is a per-channel knee. It is fine for bright UI-like scenes, but it shifts the hue of bright colors and suits realistic scenes poorly.
- Every item and every shadow caster is submitted every frame, with no culling.
- Skinning runs on the CPU (`Model::pose`). That is fine for a few characters.
- The scene's depth is discarded after the scene pass, so nothing later can read it.

**Missing.**

- Anti-aliasing (`sample_count: 1` everywhere, no FXAA or TAA).
- A graphics settings system: quality presets, render scale, shadow resolution, texture quality, capability checks.
- Shadows from spot lights or point lights.
- Screen-space ambient occlusion (only AO maps exist).
- Texture tiling and offset on materials (only `uv_tile`, studs per repeat on built-in shapes). glTF texture transforms and double-sided materials.
- Material files and material instances (materials exist only as part properties, SurfaceAppearance, or a model file's materials).
- Frustum culling, LOD, occlusion culling, per-pass GPU timing, memory accounting.
- Reflection probes, HDR environment maps, baked or probe lighting, fog, atmosphere, clouds, weather, water, stylized shading, outlines, color grading, depth of field and other effects, GPU particles, debug views.

**Render graph.** Not now. Six fixed passes in one readable function don't need one. Milestone 1's SSAO adds the first pass whose output the scene pass reads, so Phase 9 adds a small `FrameTargets` owner: it creates, resizes and frees targets by which features are on. A dependency-tracked graph is reconsidered when water (Phase 29) or TAA (Phase 35) need copies of the scene color or the last frame.

## Architectural decisions

The v1 to v3 rules and the unified plan's rules still hold. These are the ones graphics depends on.

- **Crates.** `render` stays Instance-free and takes a draw list. `scene` turns the world into that list. New render-side data (graphics settings, capabilities) is plain data in `render`. No new crate is needed for Milestones 1 to 3.
- **Two kinds of setting.** *Look* is the creator's and lives in the world: Lighting, Atmosphere and effect Instances, saved in `world.json`, set from Lua and blocks. *Cost* is the machine's: `render::GraphicsSettings`, saved per machine, never in the project folder. Where an existing Lighting property is also a cost (`ShadowsEnabled`, `ShadowDistance`), the world's value is a request and the graphics settings are a ceiling: shadows draw only if both allow them, at the smaller distance. No existing property is removed or changes meaning.
- **Today's look is the High preset.** High uses today's values (3 cascades, 2048² maps, 150 studs, bloom on), so existing worlds look the same apart from anti-aliased edges. Each new look option defaults to today's behavior for existing worlds.
- **Capabilities, not crashes.** `render::Capabilities` is read from the adapter once (MSAA sample counts for `Rgba16Float`, max texture size, timestamp queries, and later ray queries). `GraphicsSettings::effective(&Capabilities)` lowers what the machine can't do and returns a note for each change. The renderer only ever sees effective settings. Unit tests cover the lowering.
- **Targets exist only while used.** Shadows already allocate nothing when they're off. That holds for every new target: MSAA, SSAO, spot shadows, probes.
- **Materials stay additive.** `render::Material` has a `Default`, and `scene` builds it with `..Default::default()`, so a new field is never a breaking change. No shading-model enum is added until a second model exists (Phase 30).
- **New files don't bump the format.** Material files (`*.material.json`) and later probe bakes are new optional files, so `project::FORMAT_VERSION` stays 3. A changed shape of an existing file bumps it, with a migration.
- **Shaders vs scripts.** Material graphs (Phase 23) compile to WGSL and never to Lua. Gameplay graphs compile to Lua and never to WGSL. They can share the node-graph editor's look.
- **Editor-only views.** Debug views (wireframe, normals, overdraw and so on) live in the editor viewport. The player never draws them.
- **Dependencies.** None are planned for Milestone 1. Later candidates (`meshopt` for LOD, a KTX2 or EXR reader for HDR skies) are pinned in the workspace with their licences checked as Apache-2.0-compatible, and only after a phase shows the simpler path isn't enough.
- **Profile first.** Milestone 2 starts with benchmarks and per-pass timing. Occlusion culling, GPU skinning and GPU-driven drawing happen only if the numbers call for them.

## Questions for the developer

Each has a proposed answer. The phase named goes ahead with it unless told otherwise.

1. **Default anti-aliasing (Phase 4).** MSAA 4× on High where the GPU supports it, FXAA on Medium, off on Low. Existing scenes gain smoother edges.
2. **Material instances (Phase 7).** A Part or MeshPart property `MaterialVariant` holds the project path of a `*.material.json`. A material file can name a `base` file and override any of its values. A SurfaceAppearance still wins over it.
3. **Graphics settings from Lua (Phase 10).** A `UserGameSettings` service whose `GraphicsQuality` (`Enum.GraphicsQuality`: Automatic, Low, Medium, High, Ultra) and per-setting overrides LocalScripts can read and set, with matching blocks. It is saved per machine by the player. The UI phase's settings-page quality Dropdown drives it.
4. **Tone mapping (Phase 5).** Existing worlds keep `Classic`. New worlds from the editor's templates use `Neutral`.

---

# Milestone 1: Rendering foundation (brief Phase 1)

## Phase 1: A graphics reference scene

### What to build

A scene that later phases must keep rendering correctly, built before anything changes. `examples/hello-graphics` has plastic, metal, glass and Neon parts, a textured PBR MeshPart (reusing `hello-pbr`'s assets), a SurfaceAppearance part with `TileSize`, a non-uniformly sized wedge, a point light and a spot light inside lamp parts, sun shadows across all three cascades, and a bright emissive part for bloom. Its `reference.png` is the full 480×270 frame from today's `main`, so the existing example snapshot test compares the whole frame. Render tests are added for what the audit took on trust: an sRGB→linear→sRGB round trip of the draw-list colors through the real pipeline, and a non-uniformly scaled block lit like a uniform one on the face it shares.

### Acceptance criteria

- [x] `hello-graphics` loads in the player and the editor with no asset errors, and the example snapshot test matches its `reference.png`.
- [x] A mid-grey part's pixel comes back within 2 levels of its sRGB `Color` under flat white light with post-processing off.
- [x] A block stretched 4× along X has the same top-face shade as a unit block.

---

## Phase 2: Graphics settings

### What to build

`render::GraphicsSettings` holds the machine's choices: `quality` (Low, Medium, High, Ultra), plus optional overrides for shadow quality (Off, Low 1024² × 2 cascades, Medium 2048² × 2, High 2048² × 3, Ultra 4096² × 4), shadow distance (a ceiling), and post-processing (on or off). `render::Capabilities` is read from the adapter. `GraphicsSettings::effective(&Capabilities)` gives the values the renderer uses and a note for each one lowered. `Renderer::set_graphics` and `TextureRenderer::set_graphics` apply settings. A change of shadow resolution or cascade count reallocates the shadow map and frees the old one. `SHADOW_CASCADES` and `SHADOW_MAP_SIZE` become maximums. The world's Lighting values combine with the effective settings as the architectural decisions say.

The editor gets a **Graphics** section in its Settings window (`editor/src/app/settings.rs`), saved with the other editor settings, which shows what's effective and why. The player reads `graphics.json` from the game's player data folder (`save::game_folder`), defaulting to High, or Low on a software adapter (`DeviceType::Cpu`), and writes it when it changes. Reflection and ambient occlusion settings are not added here. They appear in the phase that makes them do something.

### Acceptance criteria

- [x] Each preset gives the documented values, an override replaces only its own value, and a `graphics.json` round trip keeps them (unit tests).
- [x] `effective` lowers a setting the capabilities can't meet, notes why, and never raises one (unit tests).
- [x] With shadows Off in the settings, a world with `ShadowsEnabled` on draws no shadow and allocates no shadow map. With Low, the map is 1024² with 2 cascades. A shadow distance ceiling below the world's value cuts shadows at the ceiling (GPU tests).
- [x] Switching shadow quality 50 times leaves one shadow map alive (a resource count test).
- [x] `hello-graphics` at High still matches its reference.
- [x] Changing the editor setting changes the viewport at once and survives a restart.

---

## Phase 3: Render scale and texture quality

### What to build

**Render scale** (50% to 200%, in the settings and presets: Low 75%, others 100%) sizes the HDR, depth and bloom targets. The composite pass already samples with a linear sampler, so it scales the scene to the output. The UI is always drawn at full resolution. **Texture quality** (Full, Half, Quarter: Ultra and High Full, Medium Half, Low Quarter) skips the top mip levels at upload (`mip_levels`). A change empties the texture and material caches so textures are uploaded again at the new size. `RenderStats` gains the bytes of textures and meshes on the GPU, counted at upload and when freed.

### Acceptance criteria

- [x] At 50% scale the scene targets are half size in each direction, the UI text stays sharp at full size, and picking in the editor still hits the right part.
- [x] Half texture quality uploads a 1024² texture as 512² with its smaller mips, and the reported texture bytes drop to about a quarter (GPU test).
- [x] Changing either setting at run time draws the next frame correctly with no leaked targets.

---

## Phase 4: Anti-aliasing

### What to build

An anti-aliasing setting: Off, FXAA, MSAA 2×, MSAA 4×. **MSAA** draws the HDR scene pass multisampled (color and depth) and resolves into the single-sample HDR texture that bloom reads. Every scene-pass pipeline (sky, opaque, transparent, terrain, sprites) is built for the current sample count and rebuilt when it changes. Shadow passes are unchanged. Sample counts the adapter doesn't support for `Rgba16Float` fall back through `effective`. **FXAA** runs after the composite pass on the tone-mapped image, through an intermediate LDR target that exists only while FXAA is on, before the UI. Presets: Ultra and High MSAA 4×, Medium FXAA, Low Off (question 1). TAA waits for motion vectors (Phase 35).

### Acceptance criteria

- [x] The edge of a tilted block over the sky has in-between pixel values with MSAA 4× and FXAA, and hard steps with Off (GPU test on edge pixels).
- [x] Cut-out (MASK) and see-through items, terrain, sprites and the sky draw correctly with each mode. `hello-graphics` matches its reference with MSAA (within the harness tolerance, regenerated once if edges alone differ, and that difference reviewed by eye).
- [x] An adapter that lacks MSAA 4× for the HDR format gets 2× or FXAA, with a note, and doesn't crash (capabilities unit test).
- [x] Switching modes at run time frees the multisampled targets when they're unused.

---

## Phase 5: Tone mapping and exposure

### What to build

A Lighting property `ToneMapping` (`Enum.ToneMapping`): `Classic` (today's knee, the default for existing worlds), `Neutral` (Khronos PBR Neutral: keeps base colors close to their sRGB values and rolls off highlights without shifting hue), and `Filmic` (a fitted ACES curve, for a contrasty, cinematic look). New worlds from the editor's templates use Neutral (question 4). `Exposure` keeps its meaning. The property is in the Properties panel, the Lua definitions and docs, and settable from blocks. With post-processing off, it is still a plain clamp.

### Acceptance criteria

- [x] Each curve maps 0 to 0, rises steadily, and never goes above 1. Neutral keeps a mid-grey within 2 levels (unit tests on a CPU copy of each curve, checked against the shader on a GPU test).
- [x] A world saved before this phase opens with Classic and matches its reference. A new world gets Neutral.
- [x] `game.Lighting.ToneMapping = Enum.ToneMapping.Filmic` from a script changes the next frame.

---

## Phase 6: Texture tiling, offset and double-sided materials

### What to build

`render::Material` gains `uv_scale` and `uv_offset`, applied in the vertex shader after the existing `uv_tile`, and `double_sided`. A double-sided item is drawn by pipelines without back-face culling (opaque and transparent variants), and the fragment shader flips its normal on back faces (`front_facing`). Its shadow pass draws both sides already. glTF reads `KHR_texture_transform` (offset and scale, plus rotation where its UV set matches) and `doubleSided`. FBX reads its texture's UV scale and translation. SurfaceAppearance gains tiling and offset properties, with names and value types (a Vector2 if `core` has one, otherwise U and V numbers) checked against `core` at the start of the phase and documented in `docs/rendering/models.md`. Batching keys include the new fields.

### Acceptance criteria

- [x] A glTF with a texture transform and a double-sided leaf plane draws with the transform applied and both sides lit (GPU test with a small test model).
- [x] A SurfaceAppearance with tiling 2 repeats its map twice as often, and an offset of 0.5 shifts it by half (GPU test on pixel colors).
- [x] Existing models and `hello-pbr` look unchanged, and the example snapshots still pass.

---

## Phase 7: Material files and instances

### What to build

A `*.material.json` file holds one standard material: color, metalness, roughness, emissive color and strength, opacity and cutoff, each map with its channel, inversion and strength, tiling, offset and double-sided. It can name a `base` material file. Each value it leaves out comes from the base, so an instance overrides some values without copying the rest. `assets` reads and caches material files (`Assets::material`), resolving bases with cycle and missing-file reports (once each, like textures). A Part or MeshPart property `MaterialVariant` (question 2) holds the file's project path. `scene` uses it in place of the Material preset (its Color still tints, as SurfaceAppearance does), and a SurfaceAppearance still wins. The Assets panel lists material files. A material can be made from a part's current look (**Save as Material**) and assigned by dropping it on a part. Full editing is Milestone 4.

### Acceptance criteria

- [x] A material file round-trips through save and load. An instance takes its base's values except the ones it overrides. A cycle or missing base is reported once and falls back to the default material (unit tests in `assets`).
- [x] Two parts using two instances of one base draw with their own overrides, and changing the base file changes both (GPU or draw-list test).
- [x] A world with `MaterialVariant` set saves, reloads and exports with its material files, and runs the same in the player.
- [x] Worlds with no material files load and draw as before.

---

## Phase 8: Spot light shadows

### What to build

SpotLight gains `Shadows` (default false, so existing scenes don't change). Up to N shadow-casting spot lights near the camera (settings: Low 0, Medium 2, High 4, Ultra 8) each get a layer of a spot shadow depth array (Low none, Medium 512², High 1024², Ultra 2048²), drawn with a perspective projection from the cone's angle and range. Casters outside the cone's bounding sphere are skipped. `LightRaw` gains a shadow layer index, or none. The shader filters the spot map with the same 4-texel comparison as the sun's, with a depth bias that scales with distance. Point light shadows need cube maps and come in Phase 18. `PointLight` gets no `Shadows` property until then.

### Acceptance criteria

- [x] A part between a shadow-casting spot light and a wall leaves a shadow on the wall that moves when the part moves. With `Shadows` false, there's none (GPU test).
- [x] With more shadow-casting spots than the setting allows, the nearest ones cast and the others still light without shadows.
- [x] At Low, no spot shadow array is allocated.

---

## Phase 9: Ambient occlusion

### What to build

A screen-space ambient occlusion setting (Off, Low, High; Ultra and High use High, Medium uses Low, Low uses Off). While it's on, a depth and normal prepass at render scale writes depth and view-space normals for solid items. Half-resolution SSAO (a hemisphere of 8 samples on Low, 16 on High, turned per pixel by the existing interleaved noise) reads them, and a depth-aware blur smooths the result. The scene pass multiplies it into ambient and environment light only, like AO maps, so direct sun and lamp light is unchanged. A Lighting property `AmbientOcclusionIntensity` (0 to 2, default 1) is the creator's look.

The new `FrameTargets` owns the HDR, depth, MSAA, prepass, SSAO and bloom targets. It creates and frees each by the effective settings and resizes them together. `ScenePipeline::draw` keeps its fixed pass order. The prepass's depth also serves later phases (contact shadows, fog, outlines, depth of field).

### Acceptance criteria

- [x] The inside corner where a block meets a floor is darker with AO on than off, and an open stretch of floor is within 2 levels either way (GPU test).
- [x] AO doesn't darken the sky, see-through items or emissive light. Sun-lit faces darken only in their ambient share.
- [x] With AO off, no prepass or AO targets exist and no prepass is drawn (resource count test, and the draw-call count in `RenderStats`).

---

## Phase 10: Foundation complete

### What to build

The brief's Phase 1 acceptance criteria are checked together. `UserGameSettings` (question 3) gives LocalScripts and blocks `GraphicsQuality` and the per-setting overrides. Setting them changes the player's graphics settings and saves them to `graphics.json`. Server scripts get an error. The editor's Play uses the editor's own Graphics settings. `hello-graphics` gets a LocalScript that cycles the quality with a key, and a reference image per preset (Low, Medium, High, Ultra), rendered by setting graphics settings on the test's `TextureRenderer`. The docs are updated: a new `docs/rendering/settings.md` (settings, presets, capabilities, the look-versus-cost rule), entries in `docs/rendering.md`, `pipeline.md`, `post.md`, `shadows.md`, `lights.md`, `models.md`, the scripting API docs and the `render` crate's `CLAUDE.md`. A resource test creates and drops renderers and switches every setting through every value, and checks that the caches and targets come back to their starting counts.

### Acceptance criteria

- [x] The workspace builds, tests and passes clippy. Every example still renders and matches its reference.
- [x] `hello-graphics` renders correctly at each preset and matches each preset's reference.
- [x] Lighting changes (sun direction, `EnvironmentIntensity`, a lamp's color) change metal, rough plastic, glass and textured materials consistently, compared with the reference.
- [x] Shadows, exposure, tone mapping and anti-aliasing are configurable in the editor and from a LocalScript, and each change shows in the rendered frame.
- [x] A forced-unsupported capability (no MSAA, a small max texture size) renders with fallbacks and notes, without crashing.
- [x] No GPU resource count grows after the setting-switching test.

---

# Milestone 2: Geometry performance (brief Phase 2)

## Phase 11: Benchmarks and pass timing

### What to build

Measure before optimizing. Benchmark scenes are built in code: 10,000 parts, 2,000 copies of one MeshPart, a large terrain with foliage, and 20 animated characters. Ignored tests (`cargo test -p axcenity-player --release -- --ignored bench_`) print CPU time for `build_draw_list` and `ScenePipeline::draw`, and GPU time per pass. `gpu_timer` writes timestamps per pass (`timestamp_writes` on each render pass) where the adapter allows it. `RenderStats` gains items submitted, items drawn, batches, and GPU bytes by kind. The editor's Profiler shows per-pass times.

### Acceptance criteria

- [x] Each benchmark runs and prints repeatable numbers (within 10% across three runs on one machine). The numbers are recorded in `docs/rendering/performance.md` as the baseline.
- [x] The Profiler lists the time of each pass where timestamps are supported, and says so where they aren't.

---

## Phase 12: Frustum culling

### What to build

`plan_draws` skips items outside the camera's view. Every shape fits the unit cube, so an item's bounding sphere comes from its transform alone. Each shadow cascade culls casters against its own box, stretched toward the sun by the caster range. Spot shadows cull against their cone. Sprites and terrain chunks are culled the same way. Culling counts go into `RenderStats` and editor debug overlays (a View option outlining culled bounds).

### Acceptance criteria

- [x] Items behind the camera or off to the side aren't drawn, items crossing the edge of the view are, and shadows from casters outside the view still fall into it (tests on `plan_draws` and a GPU shadow test).
- [x] The 10,000-part benchmark's frame time drops when most parts are out of view, measured against the Phase 11 baseline.

---

## Phase 13: Level of detail

### What to build

Imported models get simplified LODs at import (`meshopt`, if its licence checks out, at 50% and 25% of the triangles, cached with the model), or use the file's own `_LOD1` and `_LOD2` meshes. `MeshSource` gains optional LODs. `render` picks one by the item's size on screen, with hysteresis so it doesn't flicker. A short dithered cross-fade hides the switch. Foliage gets a cull distance with a dithered fade. Skinned models choose their LOD before posing.

### Acceptance criteria

- [x] A model far from the camera draws with fewer triangles (`RenderStats`) and switches back up as the camera approaches, without flickering at the threshold.
- [x] The repeated-MeshPart and foliage benchmarks improve over the baseline. Animated characters and terrain still draw correctly.

---

## Phase 14: GPU skinning (only if Phase 11 shows the need)

### What to build

If the 20-character benchmark shows CPU posing as a real cost, skin on the GPU: joint indices and weights become vertex attributes, each posed MeshPart uploads its joint matrices to a storage buffer, and the vertex shader blends them. Posed meshes then share one GPU copy per model, not one per part. CPU posing stays for picking and for the bind-pose collision hull.

### Acceptance criteria

- [x] A posed character looks the same skinned on the GPU as on the CPU (pixel comparison test), and the character benchmark improves.
- [ ] `examples/hello-animation` and the Animation window's preview still play correctly.

---

## Phase 15: Resource budget and occlusion (as measured)

### What to build

Textures get a GPU memory budget from the settings. Over it, the least recently drawn textures drop their top mips first. Occlusion culling (a Hi-Z pyramid from the Phase 9 prepass, testing last frame's bounds) is built only if a benchmark shows overdraw-heavy scenes that frustum culling and LOD don't fix. Otherwise this phase records that, and the reasons, in `performance.md`.

### Acceptance criteria

- [x] A scene with more texture data than the budget stays within it and draws without errors (GPU byte count test).
- [x] Occlusion culling, if built, removes hidden items in the benchmark without visible popping. If not built, the decision and its numbers are written down.

---

# Milestone 3: Advanced lighting (brief Phase 3)

## Phase 16: Environment maps

### What to build

The procedural sky is drawn into a cubemap when it changes, then prefiltered into a GGX mip chain for reflections, with spherical harmonics for diffuse light. Shading reads those in place of the analytic gradient. The analytic path stays as the Low fallback. A Lighting property can name an HDR sky image (format and reader chosen, and licence checked, at the start of the phase). The **Reflection quality** setting appears now: it controls the cubemap size and the filtering samples.

### Acceptance criteria

- [x] With the default sky, metals and rough plastics look as before within the harness tolerance. A sky image shows in reflections.
- [x] Changing the sky colors re-bakes the cubemap once, not every frame.

---

## Phase 17: Reflection probes

### What to build

A `ReflectionProbe` inside a Part (its box is the probe's reach) is baked in the editor on demand: six faces of the scene from its center, prefiltered as in Phase 16, saved as a project file. Objects inside it reflect it with box-projected (parallax-corrected) lookups, blending between overlapping probes and back to the sky at the edges. Probes are left out below Medium.

### Acceptance criteria

- [x] A metal sphere inside a red-walled room reflects red walls with a probe, and the sky without one.
- [x] Baked probes save, reload and export, and a probe whose file is missing falls back to the sky with a report.

---

## Phase 18: Point light shadows and contact shadows

### What to build

PointLight gains `Shadows`, using a cube depth map per light (six faces, budgeted by the settings like spot shadows). Contact shadows (a short screen-space ray march toward the sun and shadowing lamps, using the prepass depth) catch small gaps the shadow maps miss. They're on at High and Ultra.

### Acceptance criteria

- [x] A shadow-casting point light shadows a part on every side of it.
- [x] Contact shadows darken the gap under a small object resting on a floor, and turning them off removes only that.

---

## Phase 19: Light probe volumes

### What to build

Indirect light without lightmap UVs. A `LightProbeVolume` part holds a grid of probes, baked in the editor by rendering small cubemaps at each probe (sun, lamps, sky and one bounce) into L1 spherical harmonics, saved as a project file. Every object, still or moving, samples the grid for its ambient and diffuse environment light, so dynamic objects match their surroundings. Lightmaps are left out unless probe volumes prove too coarse for interiors.

### Acceptance criteria

- [x] Inside a lit room, a moving part picks up the color bounced off a strongly colored wall, and outside the volume falls back to the sky.
- [x] Baking is repeatable, and a stale bake (the scene changed) is flagged in the editor.

---

## Phase 20: Optional real-time GI and ray tracing

### What to build

Decided by measurement and platform support. Either probe volumes update gradually at run time (re-tracing a few probes per frame against a simplified scene), or, where `wgpu`'s ray queries are available and stable on all three platforms' drivers, ray-traced shadows and reflections as an Ultra option. Each feature is off by default and off when the capability is missing.

### Acceptance criteria

- [x] A moving sun updates indirect light within a few seconds with real-time GI on, and nothing changes with it off.
- [x] Machines without the needed capability fall back silently, with a note in the Graphics settings.

---

# Milestone 4: Material Studio (brief Phase 4)

## Phase 21: Material editor

### What to build

A Material document tab for `*.material.json`, with every value and map, the base chain shown and each overridden value marked (and resettable to the base), a live preview (sphere, cube or the selected part, through a `TextureRenderer` as the Animation preview does), presets (plastic, metal, glass, wood, stone, fabric), and **Apply** to selected parts. Edits update every part using the material at once, and undo works.

### Acceptance criteria

- [x] A material created, edited, saved, reopened and assigned looks the same in the preview, the viewport and Play.
- [x] Editing a base material updates its instances' previews and parts, except for their overridden values.

---

## Phase 22: Custom shader code

### What to build

A material can supply a WGSL `surface` function that takes the standard inputs (UV, world position, normal, time) and returns the standard outputs (base color, metalness, roughness, normal, emissive, opacity). It is inserted into the scene shader's template, checked with `naga` (already in `wgpu`), and compiled into a pipeline cached by the shader's hash. Errors are shown in the Output window and the Material tab with the material file's line numbers, and the material draws in a visible error color until fixed.

### Acceptance criteria

- [x] A custom shader that scrolls a texture over time draws in the viewport and in Play.
- [x] A syntax error is reported with the right line and doesn't stop the rest of the scene from drawing.

---

## Phase 23: Material graph

### What to build

The generic node-graph editor (`editor/src/node_graph.rs`) gets a material node set: texture sample, UV and tiling, world position, time, math, color (blend, hue shift, contrast), noise (value, Perlin, Voronoi), parameters (exposed in the Properties panel and to instances), and the material output. The graph compiles to the Phase 22 WGSL surface function. The editor stores the compiled WGSL in the material file, so the player reads only WGSL and never needs the graph code.

### Acceptance criteria

- [x] A graph mixing two textures by noise previews live, saves, and draws the same in the player.
- [x] An exposed parameter overridden in an instance changes only that instance.
- [x] Graph errors (an unconnected output, a type mismatch) point at the node.

---

# Milestone 5: Environment and atmosphere (brief Phase 5)

## Phase 24: Sun, moon and atmosphere

### What to build

Lighting gains `ClockTime` and `GeographicLatitude`, which set the sun's and moon's directions. Existing worlds keep today's fixed sun until `ClockTime` is changed. An `Atmosphere` instance under Lighting switches the sky from the gradient to physically based scattering (precomputed transmittance and sky lookup tables), with `Density`, `Offset`, `Color`, `Decay`, `Glare` and `Haze`. The sky feeds the sun's color, the environment maps (Phase 16) and the probes. Day and night transitions follow `ClockTime`, with stars and the moon at night.

### Acceptance criteria

- [x] Moving `ClockTime` from noon to dusk reddens the sun and sky and lengthens shadows. A world without Atmosphere looks as before.
- [x] Atmosphere and ClockTime save with the world and can be set from scripts and blocks.

---

## Phase 25: Fog

### What to build

Distance and height fog from Atmosphere (and Lighting's `FogStart`, `FogEnd` and `FogColor` for simple fog), applied to items, terrain, sprites and the sky using prepass depth. Volumetric fog (a froxel grid lit by the sun with its shadows, and by lamps) is an Ultra option.

### Acceptance criteria

- [x] Far parts fade into the fog color, and the fog thins with height above its offset.
- [x] Volumetric fog shows sun shafts at Ultra and is absent below it, at no cost.

---

## Phase 26: Clouds

### What to build

A `Clouds` instance (`Cover`, `Density`, `Color`) draws a ray-marched cloud layer lit by the sun, which casts moving cloud shadows on the ground. Cost scales with the settings, down to a flat cloud texture on Low.

### Acceptance criteria

- [x] Clouds move with `Workspace.GlobalWind`, and their shadows move with them.
- [x] Cover 0 shows a clear sky, and removing the instance costs nothing.

---

## Phase 27: Weather

### What to build

One `Weather` state (under Lighting) holds precipitation (none, rain, snow), intensity, wetness and snow cover. Wind is `Workspace.GlobalWind`, which already exists. Materials read it through one shared uniform: wet surfaces darken and become glossy, and snow covers upward-facing surfaces by cover and slope, on parts, models and terrain. Rain and snow are particles spawned around the camera (the existing particle system first). Changes ease over a set time. There is no per-effect environment state.

### Acceptance criteria

- [x] Turning rain on wets surfaces over time and drying reverses it. Snow cover whitens tops but not walls.
- [x] Weather saves with the world, can be scripted, and costs nothing when off.

---

## Phase 28: Wind-driven vegetation

### What to build

Foliage and marked models sway in the vertex shader by `GlobalWind`, with per-vertex sway weights (by height for built-in plants, from vertex color or a material setting for imported ones), and gust noise. Shadows sway with them.

### Acceptance criteria

- [x] Plants sway in the direction and strength of the wind, and still with zero wind.
- [x] Their shadows match their movement.

---

## Phase 29: Water

### What to build

A water material (a shading option on material files) with animated Gerstner waves, refraction (a copy of the opaque scene and its depth, taken before see-through items), reflections (the environment and probes, plus screen-space reflections at High), depth tint, and shoreline foam. This is the point where the render graph question is reopened (see the Audit).

### Acceptance criteria

- [x] A water plane over a pool shows the pool floor bent by the waves, reflects the sky and nearby parts, and tints deeper water.
- [x] Water draws correctly with fog, weather and the other Milestone 5 features on, and scales down on Low.

---

# Milestone 6: Stylized rendering (brief Phase 6)

## Phase 30: Unlit and toon shading

### What to build

Material files gain `shading`: `Standard` (the default), `Unlit`, and `Toon` (a lighting ramp from steps or a ramp texture, rim light, a hard or soft shadow edge, and a specular highlight size). All of them use the same lights, shadows and post-processing, in the same pass.

### Acceptance criteria

- [ ] Standard, Unlit and Toon parts draw side by side in one scene, each lit as its model says.
- [ ] Existing materials stay Standard and look unchanged.

---

## Phase 31: Outlines

### What to build

Per-material geometry outlines (inverted hull, with width and color) and a screen-space edge post effect (depth and normal edges from the prepass), each optional.

### Acceptance criteria

- [ ] An outlined Toon character shows an even outline at any distance, and edge detection outlines a scene without per-material setup.

---

## Phase 32: Color grading and palettes

### What to build

A `ColorCorrectionEffect` (`Brightness`, `Contrast`, `Saturation`, `TintColor`) under Lighting or the camera, LUT grading (a `.cube` file or a strip image), posterization and palette quantization. These are applied in the composite pass, after tone mapping.

### Acceptance criteria

- [ ] Each control changes the frame as expected, and a neutral LUT changes nothing (GPU test).

---

## Phase 33: Graphics volumes

### What to build

A `GraphicsVolume` part (its box, a blend distance and a priority) holds effect instances: color correction, exposure, fog and atmosphere overrides, bloom. `scene` blends the volumes around the camera into the one set of look values the draw list carries, so `render` is unchanged. These are look settings and stay separate from graphics settings.

### Acceptance criteria

- [ ] Walking into a volume blends its look in over its blend distance and out again, and overlapping volumes follow priority (unit tests on the blend, plus a visual check).

---

# Milestone 7: Post-processing and effects (brief Phase 7)

## Phase 34: Lens effects

### What to build

`DepthOfFieldEffect` (focus distance, in-focus radius, near and far intensity), vignette, film grain, chromatic aberration, and sharpening, each an effect instance with its own quality setting, all off by default.

### Acceptance criteria

- [ ] Each effect does what it says on a reference scene and costs nothing when absent.

---

## Phase 35: Motion vectors, TAA and motion blur

### What to build

Motion vectors (from this frame's and last frame's transforms, posed meshes included), TAA as a fifth anti-aliasing mode with history clamping, and motion blur from the vectors. These are the first features that keep last frame's image, so the render graph question is reopened here as well.

### Acceptance criteria

- [ ] TAA smooths edges and thin geometry without ghosting behind moving parts. Motion blur streaks only what moves relative to the camera.

---

## Phase 36: GPU particles

### What to build

Particle emitters with large counts simulate and sort on the GPU (compute), alongside the existing CPU path, keeping the same properties and look. They are used by default only above a particle count where the benchmark shows a benefit.

### Acceptance criteria

- [ ] An emitter with 100,000 particles runs, the existing particle examples look the same, and machines without compute support fall back to the CPU path.

---

# Milestone 8: Profiling and debug views (brief Phase 8)

## Phase 37: Graphics profiler

### What to build

A Graphics tab in the editor's Profiler: CPU and GPU frame time, time per pass, draw calls, triangles and vertices, culled and drawn counts, LOD distribution, GPU memory by kind, and shader compilation messages.

### Acceptance criteria

- [ ] Every number matches `RenderStats` and changes as the scene changes.

---

## Phase 38: Debug views

### What to build

Viewport View modes: lit, unlit, wireframe (drawn in the shader from barycentric coordinates, since line polygon mode isn't on every backend), normals, base color, metalness, roughness, AO, overdraw (additive count), LOD level colors, and culling bounds. These are editor only, and the player never compiles them in.

### Acceptance criteria

- [ ] Each view shows what it names on `hello-graphics`, and switching back to lit restores the normal frame.
