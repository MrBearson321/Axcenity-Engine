---
description: "A position and a rotation together."
---

# CFrame

A position and a rotation together. `part.CFrame` reads and writes the part's Position and Orientation; a CFrame isn't saved.

### Properties

#### Position

```luau
CFrame.Position: Vector3
```

Where it is.

#### X

```luau
CFrame.X: number
```

The position's X.

#### Y

```luau
CFrame.Y: number
```

The position's Y.

#### Z

```luau
CFrame.Z: number
```

The position's Z.

#### Rotation

```luau
CFrame.Rotation: CFrame
```

The same turn at the origin.

#### LookVector

```luau
CFrame.LookVector: Vector3
```

The way its front faces (its -Z), one stud long.

#### RightVector

```luau
CFrame.RightVector: Vector3
```

Its right side (its +X), one stud long.

#### UpVector

```luau
CFrame.UpVector: Vector3
```

Its top (its +Y), one stud long.

#### identity

```luau
CFrame.identity: CFrame
```

At the origin with no turn.

### Functions

#### Inverse

```luau
CFrame:Inverse(): CFrame
```

The CFrame that undoes this one: `cf * cf:Inverse()` is `CFrame.identity`.

#### Lerp

```luau
CFrame:Lerp(goal: CFrame, alpha: number): CFrame
```

The CFrame `alpha` of the way to `goal`: the position in a straight line, and the rotation the short way round.

#### ToWorldSpace

```luau
CFrame:ToWorldSpace(other: CFrame): CFrame
```

`other`, given relative to this CFrame, in world space (`cf * other`).

#### ToObjectSpace

```luau
CFrame:ToObjectSpace(other: CFrame): CFrame
```

`other`, given in world space, relative to this CFrame (`cf:Inverse() * other`).

#### PointToWorldSpace

```luau
CFrame:PointToWorldSpace(point: Vector3): Vector3
```

A point given relative to this CFrame, in world space (`cf * point`).

#### PointToObjectSpace

```luau
CFrame:PointToObjectSpace(point: Vector3): Vector3
```

A point given in world space, relative to this CFrame.

#### VectorToWorldSpace

```luau
CFrame:VectorToWorldSpace(direction: Vector3): Vector3
```

A direction given relative to this CFrame, turned into world space (the position doesn't count).

#### VectorToObjectSpace

```luau
CFrame:VectorToObjectSpace(direction: Vector3): Vector3
```

A direction given in world space, turned into this CFrame's space (the position doesn't count).

#### ToEulerAnglesYXZ

```luau
CFrame:ToEulerAnglesYXZ(): (number, number, number)
```

The angles around X, Y and Z in radians that `CFrame.fromEulerAnglesYXZ` takes: an `Orientation`, in radians.

#### new

```luau
CFrame.new(x: (number | Vector3)?, y: number?, z: number?): CFrame
```

A CFrame at a position with no turn: `CFrame.new()`, `CFrame.new(position)` or `CFrame.new(x, y, z)`.

#### lookAt

```luau
CFrame.lookAt(at: Vector3, target: Vector3, up: Vector3?): CFrame
```

A CFrame at `at` whose front (LookVector) faces `target`, with its top as close to `up` (left out: straight up) as it can be.

```lua
local turret = workspace.Turret
turret.CFrame = CFrame.lookAt(turret.Position, workspace.Target.Position)
```

#### Angles

```luau
CFrame.Angles(rx: number?, ry: number?, rz: number?): CFrame
```

A turn by these angles in radians around X, then Y, then Z, such as `CFrame.Angles(0, math.rad(90), 0)`.

#### fromEulerAnglesYXZ

```luau
CFrame.fromEulerAnglesYXZ(rx: number?, ry: number?, rz: number?): CFrame
```

The turn an `Orientation` stands for, in radians: around Z, then X, then Y.

#### fromAxisAngle

```luau
CFrame.fromAxisAngle(axis: Vector3, angle: number): CFrame
```

A turn by `angle` radians around `axis`.

