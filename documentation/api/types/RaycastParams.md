---
description: "Which parts workspace:Raycast, Spherecast, Blockcast and Shapecast find, from RaycastParams.new()."
---

# RaycastParams

Which parts `workspace:Raycast`, `Spherecast`, `Blockcast` and `Shapecast` find, from `RaycastParams.new()`.

### Properties

#### FilterDescendantsInstances

```luau
RaycastParams.FilterDescendantsInstances: {Instance}
```

These Instances and everything inside them are passed by (Exclude) or the only parts found (Include). Empty at first.

#### FilterType

```luau
RaycastParams.FilterType: EnumRaycastFilterType
```

`Enum.RaycastFilterType.Exclude` (the default) or `Include`.

#### CollisionGroup

```luau
RaycastParams.CollisionGroup: string
```

Only parts in groups that collide with this one are found. `"Default"` at first.

#### RespectCanCollide

```luau
RaycastParams.RespectCanCollide: boolean
```

If true, parts with `CanCollide` off are passed by. False at first.

#### IgnoreWater

```luau
RaycastParams.IgnoreWater: boolean
```

Kept for familiarity; there's no water yet, so it changes nothing.

#### CompensateFor

```luau
RaycastParams.CompensateFor: Player?
```

Lag compensation: on a server, `workspace:Raycast` for this Player's shot tests moving parts where that Player saw them when they fired (their round trip plus the smoothing delay, at most 1 second back); their own character is tested where it is. nil at first.

### Functions

#### AddToFilter

```luau
RaycastParams:AddToFilter(instances: Instance | {Instance}): ()
```

Adds an Instance, or a list of them, to `FilterDescendantsInstances`.

#### new

```luau
RaycastParams.new(fields: {[string]: any}?): RaycastParams
```

Params that find every part scripts can query. `fields` sets some at once: `RaycastParams.new({FilterDescendantsInstances = {character}})`.

```lua
local params = RaycastParams.new()
params.FilterDescendantsInstances = { workspace.Hero }
params.FilterType = Enum.RaycastFilterType.Exclude
local result = workspace:Raycast(workspace.Hero.Head.Position, Vector3.new(0, -50, 0), params)
if result then
	print("Standing on", result.Instance.Name)
end
```

