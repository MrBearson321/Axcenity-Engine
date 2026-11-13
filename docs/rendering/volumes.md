# Graphics volumes

A `GraphicsVolume` (graphics Phase 33) is a Part that gives a place its own look: while the camera is inside its box, the effects inside it replace that part of the world's look, and over `BlendDistance` studs outside it the look fades in and out. Walking into a cave can raise the exposure, bring in cold fog and cool the colors, then hand them back on the way out. These are look settings, saved with the world; the machine's graphics settings ([settings.md](settings.md)) are separate and untouched.

## The volume

Everything a Part has, with its own defaults: anchored, `CanCollide` and `CanQuery` off, `Transparency` 1 (invisible, so nothing of it is drawn), no shadow, 20 × 12 × 20 studs. Its box is its `Position`, `Orientation` and `Size`, turned with it.

| Property | Default | What it does |
|---|---|---|
| `Enabled` | true | Off, it changes nothing. |
| `BlendDistance` | 5 | Studs outside its faces over which its look fades in, evenly, as the camera comes closer. Inside the box the look is whole. 0 switches it on at the faces. |
| `Priority` | 0 | Where volumes overlap, the higher one wins. Equal ones follow Explorer order, the later winning. |

## The effects inside it

Each replaces one part of the world's look while the camera is there. What the volume doesn't hold stays as the world (or a lower volume) has it.

| Effect | Replaces |
|---|---|
| `ExposureEffect` (`Enabled`, `Exposure` 1) | Lighting's `Exposure`. |
| `FogEffect` (`Enabled`, `Color` light grey, `Start` 0, `End` 200, `Density` 0, `Glare` 0) | The world's fog, with distance fog. With a `Density` above 0 it's dust or mist in the volume's box instead, seen from anywhere and added to the world's fog, not replacing it ([fog.md](fog.md), Dust in a box). |
| `BloomEffect` (`Enabled`, `Intensity` 0.35, `Threshold` 1) | Lighting's bloom. It blooms even where Lighting's `BloomEnabled` is off; `Intensity` 0 turns bloom off there. |
| `Atmosphere` | The sky's air, the sun's color through it, and its height fog ([sky.md](sky.md), [fog.md](fog.md)). `ClockTime` still comes from Lighting. A `FogEffect` beside it wins for the fog. |
| `ColorCorrectionEffect` | Lighting's color grading. Several in one volume add up as they do in Lighting ([post.md](post.md)). |

Of each kind but color correction, the first enabled one in the volume is used. `ExposureEffect`, `FogEffect` and `BloomEffect` do nothing outside a volume; Lighting has its own properties for them. Lens effects (depth of field, vignette and the rest, [post.md](post.md)) work in Lighting only, and do nothing in a volume.

## Blending

`scene/src/graphics_volumes.rs` does it all while building the draw list, from the draw list's camera, so `render` only ever sees one look (`DrawList::post`, `fog`, `sky`, `sun` and `grading`). The editor viewport blends from its own camera, so flying through a volume while editing shows it too.

1. Lighting's look is worked out first, as without volumes.
2. Each enabled volume in the Workspace (not inside a `UnionOperation`) gets a weight: 1 inside its box, `1 − distance / BlendDistance` outside it, measured from the camera to the box in the box's own turned frame.
3. Those with a weight above 0 are taken from the lowest `Priority` to the highest (a stable sort, so Explorer order breaks ties). Each blends from the look so far toward that look with its effects applied, by its weight. So the higher one wins where it's whole, and blends over the lower one at its edges.

Numbers and colors blend straight between: exposure, bloom intensity and threshold, the sun's color and intensity, an Atmosphere's air over another, fog distances and height fog, and brightness, contrast, saturation and tint. Bloom that's off blends as bloom of intensity 0. Distance fog coming from none starts and ends `1 / weight` times as far away, so it closes in from the distance. What can't blend switches halfway: a lookup table, `Posterize` and a palette, the plain sky for an Atmosphere's, and a fog color where one side is the sky's.

Environment maps are re-baked when the sky changes ([sky.md](sky.md)), so an Atmosphere blending in re-bakes them each frame of the blend, and not once it settles.

## In the editor

**Graphics Volume** on the Lighting ribbon (and the Explorer's Insert menu) inserts one into the Workspace standing on the ground. **Exposure**, **Fog** and **Bloom** beside it insert their effect into the selected volume, or into a new one with none selected. **Color Correction** and **Atmosphere** go into a selected volume too, and otherwise into Lighting. Properties shows the volume's **Graphics Volume** section and each effect's **Effect** section (a `FogEffect`'s `Color` is under Appearance).

`examples/hello-volumes` has a cave with two volumes: CaveLook (brighter exposure, cold blue fog, a cool tint, stronger bloom) and, at the back, Embers (warm fog and tint at `Priority` 1). Fly in, or press V to glide in and out.

Tests: `scene` `graphics_volumes::tests` (blending in and out over the distance, priority, a turned box, fog, bloom, color correction and Atmosphere, and a world without volumes). `player` `volume_tests` (hello-volumes drawn: half the look halfway into the blend, and the higher priority winning). `editor` `app::tests::graphics_volumes_hold_the_effects_inserted_into_them`.
