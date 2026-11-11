---
description: "UI that floats in the world, parented to a Part or MeshPart and drawn on the screen UI overlay."
---

# BillboardGui

```luau
Instance.new("BillboardGui")
```

UI that floats in the world, parented to a Part or MeshPart and drawn on the screen UI overlay. Size is on-screen; StudsOffset moves it in world studs.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
BillboardGui.Class: string
```

Space-separated CSS class names used to style descendants of this billboard.

#### Id

```luau
BillboardGui.Id: string
```

The CSS identifier used to style this billboard.

#### Enabled

```luau
BillboardGui.Enabled: boolean
```

If false, this billboard is not drawn.

#### Size

```luau
BillboardGui.Size: UDim2
```

The on-screen size, in scale of the viewport plus a pixel offset.

#### StudsOffset

```luau
BillboardGui.StudsOffset: Vector3
```

World studs added to the parent part's position. The offset is not rotated with the part.

#### MaxDistance

```luau
BillboardGui.MaxDistance: number
```

The billboard hides past this many studs from the camera. 0 shows it at any distance.

