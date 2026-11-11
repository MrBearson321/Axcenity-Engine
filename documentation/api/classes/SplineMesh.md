---
description: "Inside a Spline: bends the model MeshId names along the curve, repeated to fill it with no gaps, for roads, pipes, fences and rails."
---

# SplineMesh

```luau
Instance.new("SplineMesh")
```

Inside a Spline: bends the model MeshId names along the curve, repeated to fill it with no gaps, for roads, pipes, fences and rails. Its pieces are anchored MeshParts that collide as their bent shapes, made again whenever the spline or its settings change and when the game starts; they're its children, but never saved (Bake in the editor keeps them).

It also has every member of [Instance](Instance.md).

### Properties

#### MeshId

```luau
SplineMesh.MeshId: string
```

The model bent along the Spline it's in, such as "assets/models/road.glb": a straight piece whose length runs along its Z. Empty makes nothing.

#### MaterialVariant

```luau
SplineMesh.MaterialVariant: string
```

A material file its pieces are drawn with in place of the model's materials. Empty for the model's own.

#### Size

```luau
SplineMesh.Size: Vector3
```

One piece in studs: its width (X), height (Y, up from the curve) and length (Z, along it). A whole number of pieces fills the curve, each stretched a little to fit. (8, 1, 8) by default.

#### CanCollide

```luau
SplineMesh.CanCollide: boolean
```

If true (the default), its pieces collide as their bent shapes, so cars drive and characters walk on a road.

#### CastShadow

```luau
SplineMesh.CastShadow: boolean
```

If true (the default), its pieces cast shadows.

