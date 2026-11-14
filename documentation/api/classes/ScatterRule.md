---
description: "Inside a Terrain: places plants, props or models (Models) over the ground by itself, where its height, slope, layer and water settings allow, about Spacing studs apart at Density, gathered into clumps by Clumping and keeping AvoidRadius from what the rules before it placed."
---

# ScatterRule

```luau
Instance.new("ScatterRule")
```

Inside a Terrain: places plants, props or models (Models) over the ground by itself, where its height, slope, layer and water settings allow, about Spacing studs apart at Density, gathered into clumps by Clumping and keeping AvoidRadius from what the rules before it placed. Rules work in order, like TerrainLayers. What they place is made again on every device whenever the ground or the rules change (only the parts of the terrain that changed), and is never saved; Bake in the Terrain Studio keeps it as painted foliage.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
ScatterRule.Enabled: boolean
```

If false, the rule places nothing. True by default.

#### Models

```luau
ScatterRule.Models: string
```

What it places, separated by `;`: model files such as "assets/models/pine.glb", or the built-in plants and props as "plant:Tree", "plant:Bush", "plant:Flower", "plant:Grass" and "prop:Crate". Each spot picks one. In the editor, select the rule and click props or models in the Assets panel to add or take them out.

#### Seed

```luau
ScatterRule.Seed: number
```

Which random layout it makes: the same seed, terrain and settings give the same layout on every device. 1 by default.

#### Spacing

```luau
ScatterRule.Spacing: number
```

About how far apart two of its spots are, in studs (at least 1): the terrain is cut into squares this wide, each with one spot that's used or not. 8 by default.

#### Density

```luau
ScatterRule.Density: number
```

How many it places per 100 square studs where the ground suits it, at most one per Spacing square. 1 by default.

#### MinHeight

```luau
ScatterRule.MinHeight: number
```

Nothing lower than this, in studs above the terrain's bottom. 0 by default.

#### MaxHeight

```luau
ScatterRule.MaxHeight: number
```

Nothing higher than this, in studs above the terrain's bottom (a tree line). 10000 by default.

#### HeightFade

```luau
ScatterRule.HeightFade: number
```

Studs over which it thins out at MinHeight and MaxHeight. 4 by default.

#### MinSlope

```luau
ScatterRule.MinSlope: number
```

Nothing on ground flatter than this, in degrees. 0 by default.

#### MaxSlope

```luau
ScatterRule.MaxSlope: number
```

Nothing on ground steeper than this, in degrees. 35 by default.

#### SlopeFade

```luau
ScatterRule.SlopeFade: number
```

Degrees over which it thins out at MinSlope and MaxSlope. 5 by default.

#### Layer

```luau
ScatterRule.Layer: string
```

The name of a TerrainLayer of the same terrain: it places only where that layer covers at least LayerThreshold of the ground. Empty (the default) for any ground; a name the terrain doesn't have places nothing.

#### LayerThreshold

```luau
ScatterRule.LayerThreshold: number
```

0 to 1: how much of Layer the ground must have. 0.5 by default.

#### Clumping

```luau
ScatterRule.Clumping: number
```

0 to 1: 0 spreads it evenly, 1 gathers it into clumps ClumpSize studs across with gaps between. 0 by default.

#### ClumpSize

```luau
ScatterRule.ClumpSize: number
```

Studs across its clumps. 48 by default.

#### AvoidRadius

```luau
ScatterRule.AvoidRadius: number
```

Studs it keeps from anything the ScatterRules before it (above it in the terrain) placed, such as bushes keeping clear of trees. 0 by default.

#### Underwater

```luau
ScatterRule.Underwater: boolean
```

If true, it may place under the terrain's water too. False by default.

#### RotationRange

```luau
ScatterRule.RotationRange: Vector2
```

Each is turned round its up by a random number of degrees between X and Y. (0, 360) by default.

#### ScaleRange

```luau
ScatterRule.ScaleRange: Vector2
```

Each is its own size times a random number between X and Y. (0.85, 1.15) by default.

#### CanCollide

```luau
ScatterRule.CanCollide: boolean
```

If true, parts and characters bump into what it places, as into painted foliage marked solid. False by default.

#### AlignToSlope

```luau
ScatterRule.AlignToSlope: number
```

0 to 1: how far what it places leans with the ground's slope. 0 (the default) stands straight up, as trees grow; 1 stands square to the ground, as rocks and grass on a hillside do.

#### SinkDepth

```luau
ScatterRule.SinkDepth: number
```

Studs what it places sinks into the ground (at scale 1; a bigger one sinks more), so a trunk or rock on a slope doesn't show its base. 0 by default.

#### AvoidParts

```luau
ScatterRule.AvoidParts: boolean
```

If true (the default), it places nothing under or into anchored, colliding parts standing on or near the ground (lower than 12 studs above it), such as a house's walls and floor or a placed rock. Moving such a part remakes only the ground near it.

#### PartMargin

```luau
ScatterRule.PartMargin: number
```

Studs past those parts' sides it keeps clear, with AvoidParts. 1 by default.

