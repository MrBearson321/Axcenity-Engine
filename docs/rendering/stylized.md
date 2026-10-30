# Stylized shading

Shading models (graphics Phase 30): a material file's `shading` picks how it's lit. `Standard` is physically based (the default), `Unlit` shows its color whatever the light, and `Toon` lights it in flat bands with a rim light and a sharp highlight. `examples/hello-toon` has one of each side by side.

## Material files

```json
{
  "shading": "Toon",
  "toon": { "steps": 2, "ramp": "assets/textures/ramp.png", "softness": 0.05, "rim": 0.4, "highlight": 0.25 }
}
```

- `shading`: `"Standard"`, `"Unlit"` or `"Toon"`. Left out, it comes from the base, and a file with none anywhere in its chain is Standard, so every material made before this phase looks as it did.
- `toon` (read only when the shading is Toon), merged from the base value by value, like `water`:
  - `steps` (default 3, at least 1): how many bands of light from dark to fully lit. 1 is two-tone: lit or not.
  - `ramp`: an image whose colors, left to right along its middle row, are the light from none to full, in place of `steps`. Each pixel is read whole, without blending its neighbors, so a ramp of a few pixels gives hard bands. Its left end is the light in shade, so a dark color there. `""` takes away the base's.
  - `softness` (default 0.05, 0 to 1): 0 is a hard edge between bands (and at the shadow's edge), 1 a smooth blend.
  - `rim` (default 0.3, 0 for none): how bright the rim light around the outline is.
  - `highlight` (default 0.2, 0 for none, 1 a wide one): how big the highlight is.

`assets` reads them (`Shading`, `ToonFile`, `StandardToon` in `assets/src/material.rs`; `StandardMaterial::image_paths` includes the ramp, so the loading screen reads it ahead). `scene` hands them over as `render::Material::shading` (`Shading::Standard`, `Unlit` or `Toon(Toon)`); a ramp image not loaded yet leaves the bands. The editor's Material tab has a Shading section (`material_editor.rs`, `shading_rows`).

## Drawing

Every model is drawn by the same pipelines, in the same pass, with the same lights, shadows, fog and post-processing. The model rides in the material's uniform (`MapsUniform::shading` and `toon`, matching `Maps` in shader.wgsl), so materials that differ in it get their own material binding but no pipeline of their own, and a surface shader works with any model. `fs_main` picks the model after the surface is worked out:

- **Standard**: `light_surface` (lighting.wgsl).
- **Unlit**: the surface's base color, plus its glow, then fog. No light, shadow or ambient occlusion changes it, and the weather leaves it alone.
- **Toon**: `toon_surface` (shader.wgsl). It takes the sun and each lamp of its tile through `sun_arrival`, `tile_lamps` and `lamp_arrival`, the helpers `shade_surface` uses too, so it gets the sun's, clouds', lamps' and contact shadows. Each light's share (how squarely it falls, times its shadow) goes through `toon_band`: any light at all lifts a point to the first band, each band's lower edge blends over `softness` of a band, and with a ramp the share picks a ramp pixel (`textureLoad`). Lit fully, a toon surface is as bright as a Standard one facing the light. The highlight is a disc around the half-way direction, its size from `highlight`, white in the light's color. Ambient and sky light are flat (no reflections), times ambient occlusion. The rim is where the surface turns away from the camera, lit by the sun (where it isn't shadowed) and the sky, in their colors. Metalness and roughness are left out. The weather still wets and covers it.

The item shader already reads all 16 sampled textures it may, so a toon ramp is bound in the specular map's place (`material_textures`), through its sRGB view (`create_material`), and a toon material has no specular map. Water and terrain ignore the shading.

## Outlines

Graphics Phase 31: two kinds, each optional and each free while unused. `examples/hello-outlines` has both: a toon character with a material outline, near and far, and plain blocks outlined by an `OutlineEffect`.

### A material's outline (inverted hull)

```json
{ "outline": { "width": 3, "color": [0.05, 0.05, 0.1] } }
```

- `width`: in pixels of a view 1080 pixels tall, scaled with the view's height and never under one pixel. Left out, it's 2; `0` draws none (and takes away the base's).
- `color`: sRGB, black by default. Drawn unlit, but fogged.

Merged from the base value by value, like `toon` (`OutlineFile`, `StandardOutline` in `assets/src/material.rs`; `scene` hands it over as `render::Material::outline`). The Material tab has an Outline section (`outline_rows`). Any shading model can have one; water, terrain and see-through items don't.

Drawing: each solid batch with an outline (`Batch::outline`) is drawn a second time right after the solid items, with `outline_pipeline` (`vs_outline`, `fs_outline` in shader.wgsl): every corner is posed and swayed as in `vs_main`, then pushed out on screen along its normal by the width in pixels (times the clip `w`, so the same width near or far). The pipeline culls nothing and `fs_outline` keeps only back faces, so the line shows just past the silhouette and the item hides the rest. The outline values ride in the material's uniform (`MapsUniform::outline`), and every surface shader's items use the standard outline pipeline. Smooth meshes (spheres, characters) get an unbroken line; a hard-edged mesh's split normals open the line at its corners, so outline those with the effect below.

### Screen-space outlines (`OutlineEffect`)

An `OutlineEffect` inside Lighting (`Enabled`, `Color` black, `Thickness` 1 in pixels of a view 1080 pixels tall, never under one pixel) draws lines along the edges of everything solid, with no per-part setup. `scene` reads the first one (`outlines.rs`) into `DrawList::edges`; disabled or absent, it's `None`. The editor inserts one from the Lighting ribbon's **Outlines** (`Insertable::OutlineEffect`).

While `edges` is set, the depth and normal prepass is drawn even with ambient occlusion off (`targets_used`), and `edges.wgsl` runs in the scene pass after the sky, before see-through items (so glass and water aren't outlined, and edges behind them show through them). Each pixel walks out along eight directions (the axes and diagonals) up to `Thickness` pixels, in at most 12 steps (thicker lines take longer steps, skipping slivers narrower than one), comparing each step with the one before it, so a thick line is as thick along every edge, fills corners, and never takes a surface's gradual slope or curve for an edge (axis-only samples at the full thickness used to notch corners and spill off parts onto the floor):

- **Depth**: nearness (1 over the view distance) changes evenly across any flat surface on screen, so each step is checked against the line through the two before it (the first step's slope comes from whichever neighbor continues the surface more evenly). Falling behind that line by more than 0.05 of the nearness (blended in by 0.1), or reaching the sky, is an edge, and the pixel is drawn. Coming nearer by as much ends that direction: the line belongs on what's in front.
- **Facing**: the next step's view-space normal, at about the same depth, turning by more than about 45° from this step's (1 − cos above 0.3) is a crease, such as a block's corner or where it meets the floor. Its line is on one side only: the one facing the camera more or, about even, the one facing up less (the part, not the floor under it).

The last pixel of the thickness is blended by how much of it the line covers. The line is blended over by its strength. Edges aren't fogged. Removing the effect frees the prepass targets again if nothing else uses them.

## Tests

`render` `renderer/tests/shading.rs`: Standard, Unlit and Toon spheres side by side in one frame (Standard shades smoothly, Unlit is its color on both sides and ignores a brighter sun, Toon has exactly its bands), the highlight and rim showing only when asked for, a ramp coloring the light, a toon floor taking a block's shadow with a hard edge and a lamp's light, and the shading picking the material binding (a Standard material keeps the plain one). `assets` `a_toon_instance_changes_its_bases_bands_value_by_value`; `scene` `a_materials_shading_reaches_the_draw_list_and_files_without_one_stay_standard`; `editor` `toon_shading_set_in_the_tab_draws_in_the_viewport_and_play_and_saves`. `player` `every_example_renders` draws `hello-toon` and `hello-outlines`.

Outlines: `render` `renderer/tests/outlines.rs`: an outlined toon sphere's outline is about 4 pixels on every side near and far (with MSAA too) and none without one, twice the width is twice as wide, and the item's own pixels don't change; an outline is one more draw call and a see-through item gets none; screen-space edges draw exactly the block's silhouette and its two creases over a grey-on-grey scene (Off, FXAA and MSAA), leave open floor and flat faces untouched, a thick line fills the block's side of its edge and leaves the floor beside it alone, even just above the floor, take the world's color, and keep the prepass alive only while set. `assets` `an_outline_instance_changes_its_bases_outline_value_by_value`; `scene` `a_materials_outline_reaches_the_draw_list_and_files_without_one_have_none`, `lightings_outline_effect_draws_edges_unless_disabled`; `editor` `an_outline_set_in_the_tab_draws_in_the_viewport_and_play_and_saves`.
