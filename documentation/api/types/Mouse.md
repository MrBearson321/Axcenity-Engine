---
description: "The mouse in the world, from Players.LocalPlayer:GetMouse()."
---

# Mouse

The mouse in the world, from `Players.LocalPlayer:GetMouse()`. LocalScripts only.

### Properties

#### X

```luau
Mouse.X: number
```

Pixels from the view's left edge.

#### Y

```luau
Mouse.Y: number
```

Pixels from the view's top edge.

#### UnitRay

```luau
Mouse.UnitRay: Ray
```

The ray from the camera through the mouse (`CurrentCamera:ScreenPointToRay`).

#### Target

```luau
Mouse.Target: Instance?
```

The part under the mouse: the first collider along UnitRay within 1000 studs, or nil.

#### Hit

```luau
Mouse.Hit: CFrame
```

Where UnitRay first meets a collider, or the point 1000 studs along it.

