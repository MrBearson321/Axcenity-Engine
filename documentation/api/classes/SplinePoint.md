---
description: "One point of the Spline it's in: where the curve passes (Position), and how it bends there (TangentMode, InTangent and OutTangent)."
---

# SplinePoint

```luau
Instance.new("SplinePoint")
```

One point of the Spline it's in: where the curve passes (Position), and how it bends there (TangentMode, InTangent and OutTangent).

It also has every member of [Instance](Instance.md).

### Properties

#### Position

```luau
SplinePoint.Position: Vector3
```

Where the curve passes, in the world. Its place among the Spline's children is its place along the curve.

#### TangentMode

```luau
SplinePoint.TangentMode: EnumSplineTangentMode | "Auto" | "Linear" | "Free"
```

How the curve bends here: `Enum.SplineTangentMode.Auto` (the default) passes through smoothly, from where its neighbours are; `Linear` heads straight for its neighbours (two Linear points are joined by a straight line); `Free` uses InTangent and OutTangent.

#### InTangent

```luau
SplinePoint.InTangent: Vector3
```

With TangentMode Free: from the point to the handle the curve arrives along, in studs.

#### OutTangent

```luau
SplinePoint.OutTangent: Vector3
```

With TangentMode Free: from the point to the handle the curve leaves along, in studs. Longer handles bend the curve further their way.

