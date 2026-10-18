# physics

Simulates Workspace Parts with rapier3d 0.36. Depends only on `core`. Design notes: `docs/architecture.md` (Physics).

- `lib.rs`: `Physics::step(world, seconds)` is frame step 3: sync from the World, then 0 to N fixed 1/60 s steps (accumulator, capped at `MAX_STEPS_PER_FRAME`), writing `Position`/`Orientation` back after each.
- **No change events**: each `Body` keeps `seen: PartState`, the part as physics last read or wrote it. A World value that differs means a script changed it (teleport, Anchored toggle, resize). Parts no longer under Workspace lose their body.
- `parts` is a `BTreeMap` so bodies are always created and visited in the same order. Keep it sorted, or runs stop being repeatable (the 30 vs 144 fps test compares exactly).
- Any outside change calls `wake_up_all`, because a sleeping part may have lost its support.
- `shapes.rs`: colliders match the render meshes (balls use the smallest side, cylinders run along X, wedges are a convex hull). Sizes are clamped to `MIN_SIZE`.
- rapier's math is `glamx` (glam 0.33), not the workspace's glam 0.34 used by `render`. Use `rapier3d::glamx` types here and convert through `core::Vector3`, never between the two glams.
- Orientation conversions use `EulerRot::YXZ` with (y, x, z) argument order, the same as `player/src/scene.rs`.
- rapier has no rolling resistance; `ANGULAR_DAMPING` stops balls rolling forever.
