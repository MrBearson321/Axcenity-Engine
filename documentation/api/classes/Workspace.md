---
description: "The service that holds the 3D world."
---

# Workspace

```luau
game.Workspace
```

The service that holds the 3D world. Parts inside it are drawn.

It also has every member of [Instance](Instance.md).

### Properties

#### GlobalWind

```luau
Workspace.GlobalWind: Vector3
```

The wind everywhere, in studs a second. It carries particles from emitters with WindAffectsDrag, pushes unanchored parts with WindAffected, carries Lighting's Clouds along (its level part), and blows the Weather's rain and snow.

#### Gravity

```luau
Workspace.Gravity: number
```

How hard unanchored parts and the characters are pulled down, in studs a second squared: 196.2 by default. Halve it and things fall slower and characters jump higher.

#### MaxSpawnedDecals

```luau
Workspace.MaxSpawnedDecals: number
```

The most decals `workspace:SpawnDecal` keeps at once on each side: 200 by default. Past it, the oldest goes first.

#### PhysicsInterpolation

```luau
Workspace.PhysicsInterpolation: boolean
```

On (the default), moving parts are drawn smoothly between physics steps, which run 60 times a second, so they don't move in steps on faster screens. What's drawn trails the simulation by under a step; scripts, touches and the network still see the stepped values. Off, parts are drawn where the last step left them.

#### StreamingEnabled

```luau
Workspace.StreamingEnabled: boolean
```

On, each device only has what's near it (world streaming): joined players' devices are sent only what's within their radius, and the rest stays off the GPU. Off by default, so a project plays exactly as before.

#### StreamingMinRadius

```luau
Workspace.StreamingMinRadius: number
```

Everything nearer than this many studs is always loaded: 64 by default.

#### StreamingTargetRadius

```luau
Workspace.StreamingTargetRadius: number
```

Things load out to this many studs, as the network and the machine allow, and unload a bit past it: 1024 by default. Never less than StreamingMinRadius.

#### CurrentCamera

```luau
Workspace.CurrentCamera: Camera
```

The camera the player sees through. LocalScripts only.

### Functions

#### Raycast

```luau
Workspace:Raycast(origin: Vector3, direction: Vector3, params: RaycastParams?): RaycastResult?
```

The first collider along `direction` from `origin`, or nil. The direction's length is how far the ray reaches. Hits sensors (`CanCollide` off) as well as solid parts and terrain, but not parts with `CanQuery` off. `params` (a `RaycastParams`) can pass by or keep to some parts, a collision group, or sensors. The result has Instance, Position, Normal and Distance.

```lua
local hit = workspace:Raycast(Vector3.new(0, 50, 0), Vector3.new(0, -100, 0))
if hit then
	print("Ground at", hit.Position, "on", hit.Instance.Name)
end
```

#### Spherecast

```luau
Workspace:Spherecast(position: Vector3, radius: number, direction: Vector3, params: RaycastParams?): RaycastResult?
```

Sweeps a ball of `radius` from `position` along `direction` (its length is how far) and returns the first part it meets, as `Raycast` does, or nil. Parts it starts inside aren't hit.

#### Blockcast

```luau
Workspace:Blockcast(cframe: CFrame, size: Vector3, direction: Vector3, params: RaycastParams?): RaycastResult?
```

Sweeps a box of `size` placed at `cframe` along `direction` and returns the first part it meets, as `Raycast` does, or nil. Parts it starts inside aren't hit.

#### Shapecast

```luau
Workspace:Shapecast(part: Instance, direction: Vector3, params: RaycastParams?): RaycastResult?
```

Sweeps `part`'s own shape from where it is along `direction` and returns the first other part it meets, as `Raycast` does, or nil. The part doesn't move.

#### GetPartBoundsInBox

```luau
Workspace:GetPartBoundsInBox(cframe: CFrame, size: Vector3, params: OverlapParams?): {Instance}
```

The parts whose bounding boxes overlap the box of `size` at `cframe`, in Explorer order. Terrain isn't listed.

#### GetPartBoundsInRadius

```luau
Workspace:GetPartBoundsInRadius(position: Vector3, radius: number, params: OverlapParams?): {Instance}
```

The parts whose bounding boxes are within `radius` studs of `position`, in Explorer order. Terrain isn't listed.

```lua
for _, part in workspace:GetPartBoundsInRadius(Vector3.new(0, 0, 0), 10) do
	print(part.Name, "is near the middle")
end
```

#### GetPartsInPart

```luau
Workspace:GetPartsInPart(part: Instance, params: OverlapParams?): {Instance}
```

The parts whose shapes overlap `part`'s, not counting it, in Explorer order: what's inside a trigger zone. Terrain isn't listed.

#### SpawnDecal

```luau
Workspace:SpawnDecal(texture: string, cframe: CFrame, size: Vector3 | number, lifetime: number?): Instance
```

Places a ProjectedDecal of the picture `texture` (a project path, such as "assets/textures/hole.png") at `cframe`, facing its LookVector, `size` studs (a Vector3: width, height, depth; or a number for a cube), and returns it. It fades out over the last second of `lifetime` seconds (10 if left out) and is then destroyed. Past the Workspace's MaxSpawnedDecals, the oldest goes first. For bullet holes: `workspace:SpawnDecal("assets/textures/hole.png", CFrame.lookAt(hit.Position, hit.Position + hit.Normal), 0.5, 20)`.

