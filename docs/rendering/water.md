# Water

Water surfaces (graphics Phase 29): animated waves, what's under seen through the water and bent by its waves, reflections, depth tint and shoreline foam. `examples/hello-water` is a pool with a shallow and a deep end.

## Making water

A material file with a `water` block is a water material (`docs/rendering/models.md` has the rest of the format):

```json
{
  "color": [0.85, 0.97, 1.0],
  "water": { "deepColor": [0.02, 0.2, 0.28], "clarity": 3, "waveHeight": 0.12, "waveLength": 9, "waveDirection": 30, "foam": 0.4, "refraction": 0.35 }
}
```

- `deepColor` (sRGB): the color the water scatters, which deep water shows. Default `[0.02, 0.16, 0.22]`.
- `clarity`: studs of water before only about a third of what's under still shows (e⁻¹). Default 4.
- `waveHeight` and `waveLength`: the biggest wave's height above rest and its length crest to crest, in studs (defaults 0.25 and 12). Six smaller waves ride on it, each shorter, lower and turned from the last; the first four move the surface, all seven shape its normal.
- `waveSpeed`: 1 rolls at the speed real waves of that length would (with gravity of 35 studs/s², about real gravity at 0.28 m a stud), 0 stills them. `waveDirection`: degrees, 0 toward +X, 90 toward +Z.
- `foam`: how many studs of depth (straight down) the foam reaches where the water meets something; 0 is none. Default 0.5.
- `refraction`: how far the waves bend what's under, in studs. Default 0.3.

`"water": {}` is water at every default. An instance changes the values one by one (`WaterFile::over`); the material's `color` tints what's seen through the water (a Part's `Color` tints it further), and its maps, metalness, roughness and any `shader` are ignored. The Material tab has a **Water** section (a switch and a row per value).

A Part with a water material's `MaterialVariant` draws as water: the top of its box, on `render::water_surface()`, a 64×64 grid (`WATER_GRID`) the part's transform stretches, so the corners can move. Make the part's top the water's surface. Its `Transparency` fades the water out over what's behind it. Water casts no shadow, isn't in the prepass and isn't picked differently (it's still the part's box). A MeshPart can use a water material too, but its own mesh is drawn, so its waves show only where it has corners.

## Drawing

`render::Material::water` (`render::Water`) marks an item. Its values ride in the material uniform's surface parameters (`renderer/water.rs`, `water_params`), so water materials differ in their binding keys and never in pipelines. `plan_draws` puts water with the see-through items, first among them (nearest last), so it's drawn after solid items and the sky and before glass and sprites.

How much is drawn follows the **reflection quality** setting (`WaterMode::of`):

| Reflection quality | Water |
|---|---|
| Low | Blended over the scene, with no copy: reflections of the sky, sun and lamps, and its deep color. No depth tint, no bending, no foam. |
| Medium | The scene pass stops after the sky, a full-screen pass copies the solid scene (`water_copy.wgsl`: light in rgb, each pixel's distance from the camera in alpha, the sky at 65000), and a second scene pass draws the water and the rest. What's under is seen through the copy, bent by the waves, tinted by depth, with foam. |
| High, Ultra | Medium plus reflections of what's on the screen. |

The copy (`SceneCopy`, Rgba16Float at the scene's size) exists only while water is drawn from Medium up, and is freed when there's none. With MSAA the solid pass keeps its samples and depth for the second pass (`begin_scene_pass`), and the copy reads the depth's first sample (the same shader with a multisampled depth binding). The copy is bound to the item shader in ambient occlusion's place (frame binding 7, `water_frame_bind_group`), which see-through surfaces never read, so the shader stays within wgpu's 16 sampled textures. The frame uniform's `reflection_probes.y` says the water reflects the screen, `.z` that it sees through the copy.

`water.wgsl` (after shader.wgsl):

- `vs_water` moves each corner by the first four Gerstner waves (`water_waves`): round in circles along each wave, so crests sharpen. Waves shorter than four grid cells fade out, so a big plane's coarse grid doesn't jitter.
- `fs_water` works out the normal from all seven waves at the point's rest position, fading waves shorter than a few pixels (by `fwidth`), so distant water doesn't sparkle. What it reflects is `shade_surface` with a black, smooth (0.04) surface reflecting 2% head-on: the sun's and lamps' glints with their shadows, cloud shadows, the sky, reflection probes and light probe volumes. The weather isn't applied, so snow never covers water. At High a screen-space reflection marches the reflected ray through the copy (24 growing steps, then halving to the crossing) and takes the place of the sky's where it hits, fading at the screen's edges, the ray's end, and for a ray that came out far behind what it crossed; a ray dipping under the surface isn't marched.
- What's under: the copy at the point bent by the waves (less where shallow), or straight on if the bent point is in front of the water. Through `depth` studs of water (along the view), a share `e^(−depth / clarity)` of it shows, tinted by the item's color, and the deep color, lit as a matte surface facing up, fills the rest. That is added to the reflection by the share not reflected.
- Foam where the depth straight down is under `foam` studs, in a drifting noise pattern, lit like the deep color but white.
- Fog is applied last, at the surface's distance. What's under was already fogged to its own distance in the copy, so its share is fogged twice; this shows only in heavy fog.

Glass and other see-through items under water aren't in the copy, and water writes depth, so they're hidden under it from Medium up. Water seen from below isn't drawn (its back faces are culled).

**Render graph** (the Audit in `plans/axcenity-engine-graphics.md` reopens it here): still not built. Water adds one optional copy between two halves of the scene pass, decided once per frame by `WaterMode`, and the copy is owned like volumetric fog's grid (made and freed by use). The frame stays one readable function with a fixed order. TAA (Phase 35), the first feature that keeps last frame's image, reopened it and kept the same answer ([temporal.md](temporal.md)).

## Ray traced reflections (LE5)

With hardware ray tracing on (the device has ray queries: Settings > Graphics > Hardware ray tracing), water reflecting the screen (High and Ultra) also traces one ray per pixel along the reflected way wherever the screen's reflection isn't sure of its hit (`traced_reflection`, `water_rays.wgsl`): so what's above or behind the view, or hidden on screen, shows in the water. The screen's reflection still wins where it finds something (it has each surface's full shading); the ray's hit is lit the way ray traced GI lights its hits: the item's color (its texture's average) and glow, the sun with a shadow ray, the first 8 lamps with theirs, the ambient light and the light probes, facing back along the ray (rays don't give the triangle's normal), then fogged on its way back. Rays that find nothing within the ray tracing distance leave the sky's reflection. Water draws through its own pipeline (`TracedWater`, the shader with `enable wgpu_ray_query;` and `water_rays.wgsl` in `water_no_rays.wgsl`'s place) whose fourth bind group is the TLAS, the surfaces' buffer (kept while water traces, as for GI) and a small uniform; without ray queries nothing changes. See-through items (glass, other water) aren't seen by the rays, and cut-outs are seen as solid cards.

## Cost

Nothing without water: no target, pass or pipeline is used. From Medium up, with water: one target at the scene's size (8 bytes a pixel), one full-screen pass, and the scene pass split in two. The water grid is 8,192 triangles per water part.

## Tests

- `render/src/renderer/tests/water.rs` (GPU): through still water the shallow floor and its stripes show, and 8 studs down it's darker and blue; still water doesn't change over time, and waves change what's seen through it. The water reflects a blue sky; at High it reflects a red block standing in it, and at Medium it doesn't. Snow doesn't whiten deep water, white fog lightens it, and everything at once (an atmosphere's sky, distance and volumetric fog at Ultra, clouds, rain, and wind swaying a reed in the water, over waves) draws it tinted. Each preset picks its mode; at Low water is blended with no copy and no target bytes; from Medium one copy lives while water is drawn and none after, through repeated switching. Water is batched after solid items and before glass, and casts no shadow.
- `render/src/renderer/water.rs`: water's values reach the uniform, never match a plain material, and broken numbers are made finite. `render/src/water.rs`: the grid is the top of the unit box, every triangle facing up.
- `assets/src/material.rs`: `water` round-trips, an instance changes its base's waves value by value, and `{}` is the defaults. `scene/src/material.rs`: a part with a water material draws as water on the grid.
- `player` `every_example_renders` draws `hello-water`.
- `render/src/renderer/tests/water.rs` `ray_traced_water_reflects_what_is_off_the_screen` (ray queries only): a glowing block above the frame shows in the water at High with rays, not without ray queries, and not at Medium, which traces nothing.
