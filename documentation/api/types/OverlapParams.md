---
description: "Which parts workspace:GetPartBoundsInBox, GetPartBoundsInRadius and GetPartsInPart list, from OverlapParams.new()."
---

# OverlapParams

Which parts `workspace:GetPartBoundsInBox`, `GetPartBoundsInRadius` and `GetPartsInPart` list, from `OverlapParams.new()`.

### Properties

#### FilterDescendantsInstances

```luau
OverlapParams.FilterDescendantsInstances: {Instance}
```

These Instances and everything inside them are passed by (Exclude) or the only parts listed (Include). Empty at first.

#### FilterType

```luau
OverlapParams.FilterType: EnumRaycastFilterType
```

`Enum.RaycastFilterType.Exclude` (the default) or `Include`.

#### CollisionGroup

```luau
OverlapParams.CollisionGroup: string
```

Only parts in groups that collide with this one are listed. `"Default"` at first.

#### RespectCanCollide

```luau
OverlapParams.RespectCanCollide: boolean
```

If true, parts with `CanCollide` off are passed by. False at first.

#### MaxParts

```luau
OverlapParams.MaxParts: number
```

The most parts listed; 0 (the default) lists them all.

### Functions

#### AddToFilter

```luau
OverlapParams:AddToFilter(instances: Instance | {Instance}): ()
```

Adds an Instance, or a list of them, to `FilterDescendantsInstances`.

#### new

```luau
OverlapParams.new(fields: {[string]: any}?): OverlapParams
```

Params that list every part scripts can query. `fields` sets some at once: `OverlapParams.new({MaxParts = 10})`.

