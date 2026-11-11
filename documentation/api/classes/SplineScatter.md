---
description: "Inside a Spline: places the models Models names along the curve every Spacing studs, or inside a Closed spline at Density, each turned, scaled and moved at random from Seed and stood on the surface below (none where it's steeper than MaxSlope)."
---

# SplineScatter

```luau
Instance.new("SplineScatter")
```

Inside a Spline: places the models Models names along the curve every Spacing studs, or inside a Closed spline at Density, each turned, scaled and moved at random from Seed and stood on the surface below (none where it's steeper than MaxSlope). What it places is made again whenever the spline or its settings change and when the game starts; it's its children, but never saved (Bake in the editor keeps it).

It also has every member of [Instance](Instance.md).

### Properties

#### Models

```luau
SplineScatter.Models: string
```

The model files it places, separated by `;`, such as "assets/models/tree.glb; assets/models/rock.glb". Each spot picks one. In the editor, click models in the Assets panel while it's selected to add or take them out.

#### Seed

```luau
SplineScatter.Seed: number
```

Which random layout it makes: the same seed, spline and settings give the same layout every time. 1 by default.

#### Spacing

```luau
SplineScatter.Spacing: number
```

Studs between models along the curve (at least 0.5). 8 by default.

#### FillInside

```luau
SplineScatter.FillInside: boolean
```

If true, it fills the inside of a Closed spline at Density instead of placing models along the curve. False by default.

#### Density

```luau
SplineScatter.Density: number
```

Models per 100 square studs inside a Closed spline, with FillInside. 1 by default.

#### RotationRange

```luau
SplineScatter.RotationRange: Vector2
```

Each model is turned round its up by a random number of degrees between X and Y. (0, 360) by default.

#### ScaleRange

```luau
SplineScatter.ScaleRange: Vector2
```

Each model is its own size times a random number between X and Y. (1, 1) by default.

#### Offset

```luau
SplineScatter.Offset: number
```

Each model along the curve is moved up to this many studs to one side or the other at random. 0 by default.

#### AlignToSurface

```luau
SplineScatter.AlignToSurface: boolean
```

If true (the default), each model stands out from the surface below it, tilted with the ground; if false, upright.

#### MaxSlope

```luau
SplineScatter.MaxSlope: number
```

Ground steeper than this many degrees gets no model. 35 by default.

#### CanCollide

```luau
SplineScatter.CanCollide: boolean
```

If true (the default), the models it places collide.

