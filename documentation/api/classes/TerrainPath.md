---
description: "Inside a Spline: a road or path on the terrain (procedural worlds PW4), Width studs across along the Spline's curve."
---

# TerrainPath

```luau
Instance.new("TerrainPath")
```

Inside a Spline: a road or path on the terrain (procedural worlds PW4), Width studs across along the Spline's curve. ScatterRules keep off it (ClearScatter) wherever it goes, on every device, without changing the terrain. In the editor, Apply to Terrain flattens the ground under it to the curve and paints its Layer, blending in over Falloff (one undo step), and Route lays the Spline's points over the terrain from its first point to its last, no stretch climbing more than MaxGrade degrees, so a road winds round or up a hill (one undo step). Put a SplineMesh in the same Spline for the road's surface.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
TerrainPath.Enabled: boolean
```

If false, scatter rules don't keep off it and the editor's Apply and Route do nothing. True by default.

#### Width

```luau
TerrainPath.Width: number
```

Studs across the road or path. Apply flattens and paints this strip fully. 8 by default.

#### Falloff

```luau
TerrainPath.Falloff: number
```

Studs past each edge over which Apply's flattening and paint blend into the ground around. 4 by default.

#### Flatten

```luau
TerrainPath.Flatten: boolean
```

If true (the default), Apply levels the strip to the height of its Spline's curve.

#### Layer

```luau
TerrainPath.Layer: string
```

The name of the TerrainLayer Apply paints the strip with. Empty (the default) paints nothing.

#### ClearScatter

```luau
TerrainPath.ClearScatter: boolean
```

If true (the default), the terrain's ScatterRules place nothing within Width / 2 + ClearMargin studs of the curve, seen from above. They keep off it as soon as it's there and wherever its points move, on every device.

#### ClearMargin

```luau
TerrainPath.ClearMargin: number
```

Studs past each edge that scatter rules keep clear, so trees don't stand on the verge. 1 by default.

#### MaxGrade

```luau
TerrainPath.MaxGrade: number
```

The steepest the editor's Route lets the road climb, in degrees. 8 by default.

