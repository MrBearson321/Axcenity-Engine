# Wind and swaying plants

Plants bend in the wind in the vertex shader (graphics Phase 28). The wind is the Workspace's `GlobalWind` (studs a second), the same wind that blows particles, rain and snow and carries the clouds. Nothing about it is saved apart from `GlobalWind` and a material file's `sway`.

## What sways

- **Built-in plants** painted on terrain (`scene/src/foliage.rs`): grass and flowers give most (strength 1 and 0.8), bushes less (0.35), trees least (0.2, the prop tree too). Other props stay still. A plant's pieces (a tree's trunk and crown) share one `Sway`, from its ground to the top of its highest piece, so they bend as one.
- **Parts and MeshParts with a material file** whose `sway` is above 0 (`docs/rendering/models.md`): they sway by height from the bottom of the part's box to its top, with that strength. The Material tab has a **Sway** slider (0 to 2). Instances inherit it from their base like other values.
- **Models that say so**: a glTF material's `extras` can hold `axcenity_sway` (0 to 2, as a material file's `sway`) and `axcenity_flutter` (studs, up to 0.95). They work on a model's own materials, painted on a terrain or on a MeshPart; a MeshPart's `MaterialVariant` replaces them with its file's `sway`. Every piece of the model bends by height over the bottom of the model's box, so give a tree's bark and leaves the same `axcenity_sway` and only the leaves an `axcenity_flutter`. Files without these keys stay still, and older copies of the engine ignore them.
- FBX materials don't sway. Vertex colors aren't read yet, so a model can't weight its sway per vertex; it sways by height.

## How

`render::DrawItem::sway` (`Sway { base, height, strength }`, `None` to keep still) goes into the item's instance data (`InstanceRaw::sway`, location 15), and `DrawList::wind` goes into the frame uniform's spare `clock.yzw`. `swayed` in shader.wgsl moves each corner in world space after skinning:

- Its weight is its height over `base` as a share of `height`, squared, so the foot stays put and the top moves most.
- It leans downwind along the wind's level part, at most `SWAY_LEAN` (a quarter) of its height times its strength, reaching about two thirds of that at 20 studs a second (`1 − e^(−speed / SWAY_WIND)`). A vertical wind doesn't bend plants.
- Gusts (`cloud_noise2` over the ground, rolling with the wind) vary the lean between 0.7 and 1.2 of it, and a flutter (a sine whose phase varies smoothly over the ground) adds ±0.15. Both vary smoothly with world position, so pieces that touch stay together.
- The top drops a little as it bends, so a plant keeps its length.
- A flutter (`Sway::flutter`, in the fraction of `InstanceRaw::sway.w`, whose whole part is the decal role) shivers each corner by that many studs, across and along the wind and a little upward, in quick waves whose phase comes from where the corner rests, so neighbouring leaves don't match. Gusts drive it, and the bottom quarter of the plant fades to none.
- With no wind (or a NaN one), nothing moves: the frame is exactly the same as with no sway.

The scene and prepass pipelines (`vs_main`) and the shadow pipeline (`vs_shadow`) call the same function, so shadows and ambient occlusion move with the plant. The shadow passes bind the frame uniform for it (binding 0 of their group). Culling grows a swaying item's bounding sphere by `Sway::reach`, the farthest any corner can move, so a plant just out of view that can lean into it is still drawn. Normals aren't bent.

Time is `DrawList::time`. The editor keeps drawing frames while the wind blows something that sways (`viewport::shader_time`).

## Cost

One more instance attribute per item (16 bytes) and a few instructions per corner for items that sway. No pass, target or draw call is added. Items that don't sway skip it on one test.

Measured on Temperate Dry Patch at Ultra 2560 × 1440 (`profile_project`'s `no-wind` and `no-sway`, three cameras; [performance.md](performance.md)): the wind costs 0.01 to 0.14 ms of a 9 ms GPU frame, inside the run-to-run noise. There's nothing per plant on the CPU and nothing updated per object: one `GlobalWind` goes into the frame uniform, and the vertex shader works out each corner's lean, gust and flutter from its world position and the time. Plants out of view, or past their cull distance, aren't drawn, so they cost nothing at all; shadows sway in the shadow pass from the same function, so a plant coming into view or into a cascade is already where it should be. Distance-based wind quality, slower updates far away and visibility-aware simulation were considered and not built, since there's no simulation to schedule and the shader's share of the frame can't be measured apart from noise.

## Tests

- `render/src/renderer/tests/wind.rs` (GPU): a plant leans toward +X in a wind to +X and toward −X in one to −X, less in a breeze than in a gale, not sideways in a wind toward the camera, and its foot stays put. With no wind, or with no sway, the frame is exactly as still. Its shadow's end moves as many studs as its top does (within 35%; about 1.06 studs each in a 40-stud-a-second wind). A swaying plant just out of view isn't culled where a still one is.
- `scene/src/foliage.rs`: a tree's trunk and crown share one sway from its ground to its top, grass gives more than a tree, and crates stay still.
- `scene/src/material.rs`: a material file's `sway` sways a part from the bottom of its box by its height, one without doesn't, and `GlobalWind` reaches `DrawList::wind`.
- `assets/src/material.rs`: `sway` round-trips and an instance takes its base's.
- `assets/src/model/tests.rs`: a glTF material's `extras` give its sway and flutter, kept within their limits.
- `scene/src/material.rs`: a model's own material sways it from the bottom of its box. `render/src/renderer/tests/wind.rs`: leaves flutter at the top of a plant and not at its foot.
