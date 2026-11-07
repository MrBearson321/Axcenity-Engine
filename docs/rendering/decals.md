# Decals

Pictures laid on surfaces (Phase 75): a Part's `Decal` on one face, and a `ProjectedDecal` box that paints whatever solid surfaces are inside it. `render` receives both in the draw list only; `scene` makes them from the Instances.

## Decal (a Part's face)

A `Decal` inside a part puts its `Texture` on the face its `Face` names (`Enum.NormalId`: Front is -Z, then Back, Left, Right, Top, Bottom), tinted by `Color3` and faded by `Transparency`. With `StudsPerTileU` and `StudsPerTileV` at 0 the picture is stretched once across the face; above 0, one copy covers that many studs and it repeats (`Material::uv_scale`). Seen from outside, the picture is upright and unmirrored: on the side faces its top is +Y; on Top its top is toward -Z (Front), on Bottom toward +Z.

`scene/src/decals.rs` draws it as a `DrawItem` on `render::decal_quad()` (a 1×1 square facing +Z) laid over the face of the part's box (`Size`, whatever its `Shape`), 0.002 studs off it, with `DecalRole::Decal`. The renderer then:

- draws it with the see-through items, so the texture's alpha blends even at `Transparency` 0, and it casts no shadow;
- moves its depth toward the camera by `DECAL_DEPTH_BIAS` (shader.wgsl, `vs_main`; 1e-6 of clip-space depth), so it never fights the face beneath for depth, near or 600 studs away;
- leaves it out of the prepass, so projected decals don't land on it.

Face decals are drawn at every graphics quality.

## ProjectedDecal

A box (`Position`, `Orientation`, `Size`) whose picture faces its front, -Z (a CFrame's `LookVector`), and is projected along Z through the box. `Size` is the picture's width and height, then the box's depth. The default `Orientation` (90, 0, 0) faces up, so a new one lies on the ground. `FadeAngle` (75 by default) is how far a surface may turn from the picture's facing before it's gone: fully drawn to half that angle, fading out over the rest. `IgnoreCharacters` (on by default) leaves characters' parts bare.

`scene` hands each one to the renderer as a `render::Decal` in `DrawList::decals` (`projected_decal`). Parts inside anything that holds a character controller or a Humanoid get `DecalRole::Character` (`Characters`).

Drawing (`render/src/renderer/decals.rs`, `decal.wgsl`):

1. The prepass draws solid items' depth and view-space normals; a character's write 0.5 in the normal's alpha instead of 1 (`fs_prepass`, from `InstanceRaw::sway.w`, the decal role). While the list has decals the settings draw, the prepass is kept alive (`ScenePipeline::decals`, `targets_used`) and its depth is bound to frame binding 15 (`bind_frame`).
2. In the scene pass, after the sky and before screen-space outlines and see-through items, each decal's box is drawn back faces only, with no depth test, so it covers its pixels even with the camera inside it. One instanced draw per run of decals sharing a picture.
3. Each pixel reads the prepass's depth, finds the world point there, and keeps it only if it's inside the box (`inverse` of its transform). So the decal follows any shape (terrain, spheres, stairs) with no gaps, and never fights the surface for depth, since it uses that surface's own depth. The texture's coordinates come from the point's place in the box; their gradients are worked out before anything is discarded and capped (`MAX_GRADIENT`) so edges don't blur.
4. It's faded by the angle between the prepass's normal and the picture's facing, lit by `light_surface` like any surface (roughness 0.8, ambient occlusion from the screen), fogged, and blended premultiplied over the surface.

See-through items, water and face decals aren't in the prepass, so projected decals don't land on them.

## Quality

Projected decals follow the graphics quality preset (`Quality::decal_limits`, `EffectiveGraphics::decals`). There's no separate override.

| | Low | Medium | High | Ultra |
|---|---|---|---|---|
| Projected decals drawn, nearest first | none | 128 | 512 | 1024 |
| Within | | 150 studs | 300 | 600 |

The nearest within the distance (to the box's surface) that touch the view are kept, then drawn in the list's order, so a newer decal paints over an older one. The farthest fade out over the last tenth of the distance. `RenderStats` counts `decals_drawn` and `decals_left_out`. At Low the prepass isn't kept for them.

## Spawned decals

`workspace:SpawnDecal(texture, cframe, size, lifetime)` (`script/src/decals.rs`) makes a ProjectedDecal named `SpawnedDecal` in the Workspace at `cframe`, facing its LookVector, `size` studs (a Vector3, or a number for a cube), and returns it. It fades out over the last second of `lifetime` (10 seconds if left out; over the last quarter of a shorter one) by raising its `Transparency`, then is destroyed. The Workspace's `MaxSpawnedDecals` (200) caps how many each side keeps: past it, the oldest is destroyed first. Spawned decals belong to the side, not to a script. A server's replicate like any Instance; a LocalScript's stay on its device.

For bullet holes, raycast and face the hit's normal:

```lua
local hit = workspace:Raycast(origin, direction * 200, params)
if hit then
	workspace:SpawnDecal("assets/textures/hole.png",
		CFrame.lookAt(hit.Position, hit.Position + hit.Normal), 0.6, 30)
end
```

The **spawn decal** block (Looks) is `workspace:SpawnDecal(texture, CFrame.lookAt(Vector3.new(), facing) + position, size, seconds)`.

## In the editor

Insert ▸ Effects has **Decal** (into the selected part) and **Projected Decal** (at the camera's focus point, lying on the ground). Both save in `world.json` like any Instance.

`examples/hello-decals` is the example: a dashed road marking over a hill of terrain, a puddle, an arrow Decal on a sign, a tiled Decal on a crate's top, and a Blaster Tool (press 1, click) that leaves bullet holes on the target.

## Tests

- `render/src/renderer/tests/decals.rs`: a road marking on a ball's curved top covers every pixel of its strip and nothing clearly outside, from a camera moving between frames, at Medium and High; a picture faces its LookVector unmirrored with its alpha; `IgnoreCharacters`; none at Low (and no prepass kept); a face decal shows over its face at 6, 60 and 600 studs at every quality; a decal with the camera inside its box.
- `scene/src/decals.rs`: a Decal covers each face facing out; color, transparency and tiling; a ProjectedDecal's box; which parts are a character's.
- `script/src/decal_tests.rs`: SpawnDecal's placement, fade and end, the cap, and its errors. `blocks/src/tests.rs`: the block's Lua, its round trip and a run.
- `player/src/tool_tests.rs`: 500 shots from hello-decals' Blaster leave the newest 200 holes on the target. `player/src/decal_tests.rs`: the example's decals save, reload and export, drawing the same pixels. `editor/src/app/decal_tests.rs`: Play's decals are the exported game's. `player/src/bench_tests.rs`: `bench_500_bullet_holes_in_the_graphics_reference_scene` ([performance.md](performance.md)).
