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

## Tests

`render` `renderer/tests/shading.rs`: Standard, Unlit and Toon spheres side by side in one frame (Standard shades smoothly, Unlit is its color on both sides and ignores a brighter sun, Toon has exactly its bands), the highlight and rim showing only when asked for, a ramp coloring the light, a toon floor taking a block's shadow with a hard edge and a lamp's light, and the shading picking the material binding (a Standard material keeps the plain one). `assets` `a_toon_instance_changes_its_bases_bands_value_by_value`; `scene` `a_materials_shading_reaches_the_draw_list_and_files_without_one_stay_standard`; `editor` `toon_shading_set_in_the_tab_draws_in_the_viewport_and_play_and_saves`. `player` `every_example_renders` draws `hello-toon`.
