# Clouds

A layer of cloud high in the sky, lit by the sun, whose shadows fall on the world (graphics Phase 26). `DrawList::clouds` (`render::Clouds`, `render/src/draw_list.rs`) comes from the world (`scene/src/clouds.rs`); the renderer packs it into the frame (`renderer/clouds.rs`, `CloudsRaw`, `Clouds` in `clouds.wgsl`, compiled after `fog.wgsl` into every shader that includes `lighting.wgsl`).

## The world's clouds

A `Clouds` inside Lighting (the Lighting ribbon's **Clouds**, beside Atmosphere, or the Explorer's Insert menu; it always goes into Lighting, like an Atmosphere). The first one is drawn. Without one there are none.

- `Cover` (0 to 1, default 0.5): how much of the sky is cloud. 0 is a clear sky, drawn exactly as without clouds; 1 is overcast.
- `Density` (0 to 1, default 0.7): how much light the cloud takes away (0.05 per stud at 1, `EXTINCTION`), so thicker clouds are whiter on top, darker below, and cast deeper shadows. 0 is none.
- `Color` (default white): the clouds' own color, lit by the sun and the sky.
- The Workspace's `GlobalWind` (studs a second; its level part) carries them and their shadows along. The renderer keeps how far they've gone (`CloudDrift`), moving them on by the wind times the time since the last frame (`DrawList::time`), so a new wind changes their speed, not where they are; when the time counts back (a host starting again) they stay put. Each renderer has its own drift (a probe bake starts from none). The editor keeps drawing frames while clouds are in the wind (`viewport::shader_time`).

## How they're drawn

The layer lies between `Clouds::BOTTOM` (800) and `Clouds::TOP` (1200) studs up. Above each level point, four octaves of value noise (shapes about 1600 studs across, `CLOUD_SCALE`), cut at a level set by `Cover`, say how thick the cloud is (`cloud_cover`, 0 to 1); a cloud fills the layer from the bottom up to that share, rounded on top, so thick clouds stand tall and thin ones are wisps.

- **In the sky** (`clouded_sky`, from `fs_sky`): the sky behind is dimmed by the cloud in front of it and the cloud's light is added, step by step through the layer (integrated exactly over each step), each step lit by the sun through a dual-lobe phase (a silver lining toward the sun) and by the sky's colors. Far clouds fade into the sky from 5000 to 20000 studs (`CLOUD_FAR`), and a ray is marched through at most 4000 studs of the layer (`CLOUD_MARCH`). The fog then fades the clouded sky like the sky. Clouds are part of the sky: they're drawn behind everything, so from above them the ground isn't hidden, and they aren't in the environment maps' sky.
- **Shadows** (`cloud_shadow`, in `light_surface`, so on parts, models and terrain, and in the volumetric fog's sun): the sun's light at a point is dimmed by the cloud where its path crosses a quarter of the way up the layer, its thickness there over the sun's height. A share of the light gets through even thick cloud (`cloud_light_through`: light scattered many times inside), so the ground under a cloud is dim, not black. Every cloud quality casts the same shadows.

The **cloud quality** graphics setting (`CloudQuality`, [settings.md](settings.md); `UserGameSettings.CloudQuality`) sets the sky's cost: Low a flat sheet a quarter of the way up (one look-up per pixel), Medium 16 steps lit by the cloud above each step, High 32 steps with fine 3D detail eroding the edges and 3 steps toward the sun, Ultra 64 and 6. Steps start at a per-pixel offset (`pixel_noise`) so they don't band.

## Cost

Clouds add no pass, target or draw call: they're worked out in the sky pass's pixels, and every lit pixel skips them on one uniform test while there are none (`Clouds.color.w`). The sky pass is drawn after solid items and before see-through ones, with a depth test that passes only where no solid item was drawn, so the sky (and its clouds) is worked out only for the pixels that show it.

## Tests

- `render/src/renderer/tests/clouds.rs` (GPU): the clouds overhead and their shadows on a floor move by the wind's distance (the middle row's best-matching shift is the wind's 100 studs in pixels), and stay still without wind; Cover 0 draws exactly the clear frame, clouds add no draw calls or target bytes, and taking them out restores the frame; every cloud quality draws clouds and shades the ground the same.
- `render/src/renderer/clouds.rs`: none by default or without density, each quality's steps and detail, the drift following the wind (level part only), keeping its place when the wind changes or time counts back.
- `scene/src/clouds.rs`: a new world has none; Lighting's Clouds take Cover (held to 0 to 1), Density, Color and the GlobalWind, and Cover 0 or removing it is none.
- `player/src/clouds_tests.rs` (GPU): a world's Clouds shade part of the floor, its GlobalWind moves the shadows, still air keeps them, and destroying the Clouds gives the frame without them.
- `render/src/graphics.rs`: each preset's cloud quality.
