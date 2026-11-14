---
description: "A building made from a footprint (procedural worlds PW6)."
---

# BuildingGenerator

```luau
Instance.new("BuildingGenerator")
```

A building made from a footprint (procedural worlds PW6). Standing free, its footprint is its own box (Position, Size, and its turn round Y from Orientation, front facing -Z), which the editor's Move, Rotate and Scale handles change; inside a Spline, the Spline's points are its corners. On a foundation Size.Y studs thick, it stacks Floors storeys FloorHeight studs tall of wall bays about BayWidth studs wide, each a window (WindowChance of them) or plain wall, with a doorway in the middle of the front, under a Flat or Gable roof. Bays, the door and corners are built-in parts unless WallModel, WindowModel, DoorModel or CornerModel name a model to stretch into place. The same Seed gives the same building. Its parts are made again on every device whenever its settings change and are never saved; Bake in the editor keeps them as a Model.

It also has every member of [Instance](Instance.md).

### Properties

#### Seed

```luau
BuildingGenerator.Seed: number
```

Changes which bays get windows. The same Seed and settings give the same building. 1 by default.

#### Position

```luau
BuildingGenerator.Position: Vector3
```

Standing free: the middle of its foundation, in studs. Ignored inside a Spline.

#### Orientation

```luau
BuildingGenerator.Orientation: Vector3
```

Standing free: only its turn round Y (degrees) counts, so the building stays upright; its front (with the door) faces -Z turned by it. Ignored inside a Spline.

#### Size

```luau
BuildingGenerator.Size: Vector3
```

The foundation: width (X), thickness (Y) and depth (Z) in studs. Inside a Spline only the thickness counts, under the lowest point. 24, 1, 16 by default.

#### Floors

```luau
BuildingGenerator.Floors: number
```

How many storeys, 1 to 50. 2 by default.

#### FloorHeight

```luau
BuildingGenerator.FloorHeight: number
```

Studs from one floor to the next. 10 by default.

#### RoofStyle

```luau
BuildingGenerator.RoofStyle: EnumBuildingRoofStyle | "Flat" | "Gable"
```

`Enum.BuildingRoofStyle.Gable` (the default) puts a pitched roof over a rectangular footprint, its ridge along the longer side; `Flat`, or any footprint that isn't a rectangle, a flat one.

#### RoofHeight

```luau
BuildingGenerator.RoofHeight: number
```

Studs from the eaves to a gable roof's ridge. 6 by default.

#### BayWidth

```luau
BuildingGenerator.BayWidth: number
```

About how many studs of wall each window, door or plain bay takes: each side is cut into a whole number of equal bays. 8 by default.

#### WindowChance

```luau
BuildingGenerator.WindowChance: number
```

The share of bays (0 to 1) that get a window rather than plain wall, picked by Seed. 0.7 by default.

#### WallThickness

```luau
BuildingGenerator.WallThickness: number
```

Studs. 1 by default.

#### WallModel

```luau
BuildingGenerator.WallModel: string
```

A model file stretched over each plain wall bay (its front, -Z, facing out). Empty (the default) uses a built-in part.

#### WindowModel

```luau
BuildingGenerator.WindowModel: string
```

A model file stretched over each window bay. Empty (the default) builds the window from parts with a glass pane.

#### DoorModel

```luau
BuildingGenerator.DoorModel: string
```

A model file stretched over the door bay. Empty (the default) leaves an open doorway in parts.

#### CornerModel

```luau
BuildingGenerator.CornerModel: string
```

A model file stretched over each corner, one a floor. Empty (the default) uses a built-in post.

#### WallMaterial

```luau
BuildingGenerator.WallMaterial: EnumMaterial | "Plastic" | "SmoothPlastic" | "Wood" | "WoodPlanks" | "Marble" | "Granite" | "Concrete" | "Slate" | "Brick" | "Fabric" | "Sand" | "Metal" | "Glass" | "Neon" | "Ice" | "Rubber"
```

The walls' Material. Brick by default.

#### WallColor

```luau
BuildingGenerator.WallColor: Color3
```

The walls' color.

#### TrimColor

```luau
BuildingGenerator.TrimColor: Color3
```

The floors, corners, sills and lintels' color.

#### RoofColor

```luau
BuildingGenerator.RoofColor: Color3
```

The roof's color.

#### CanCollide

```luau
BuildingGenerator.CanCollide: boolean
```

If true (the default), its parts are solid.

