---
description: "Where workspace:Raycast (or a shape cast) first met a collider, or nil when it missed."
---

# RaycastResult

Where `workspace:Raycast` (or a shape cast) first met a collider, or nil when it missed.

### Properties

#### Instance

```luau
RaycastResult.Instance: Instance
```

The Part, MeshPart or Terrain the ray hit.

#### Position

```luau
RaycastResult.Position: Vector3
```

The point of contact, in studs.

#### Normal

```luau
RaycastResult.Normal: Vector3
```

The surface direction at the hit, pointing out.

#### Distance

```luau
RaycastResult.Distance: number
```

How far along the ray the hit is, in studs.

