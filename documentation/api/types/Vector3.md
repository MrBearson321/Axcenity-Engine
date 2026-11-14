---
description: "A position, size or direction in studs."
---

# Vector3

A position, size or direction in studs. Y is up.

### Properties

#### X

```luau
Vector3.X: number
```

The X component.

#### Y

```luau
Vector3.Y: number
```

The Y component (up).

#### Z

```luau
Vector3.Z: number
```

The Z component.

#### Magnitude

```luau
Vector3.Magnitude: number
```

The length of the vector.

```lua
local offset = workspace.Player.Position - workspace.Goal.Position
if offset.Magnitude < 10 then
	print("Almost there!")
end
```

#### Unit

```luau
Vector3.Unit: Vector3
```

The same direction with length 1 (the zero vector stays zero).

```lua
local direction = (workspace.Target.Position - workspace.Cannon.Position).Unit
workspace.Ball.AssemblyLinearVelocity = direction * 80
```

#### zero

```luau
Vector3.zero: Vector3
```

(0, 0, 0).

#### one

```luau
Vector3.one: Vector3
```

(1, 1, 1).

#### xAxis

```luau
Vector3.xAxis: Vector3
```

(1, 0, 0).

#### yAxis

```luau
Vector3.yAxis: Vector3
```

(0, 1, 0): up.

#### zAxis

```luau
Vector3.zAxis: Vector3
```

(0, 0, 1).

### Functions

#### Dot

```luau
Vector3:Dot(other: Vector3): number
```

The dot product: the lengths multiplied, times the cosine of the angle between them.

```lua
local door = workspace.Door
local toPlayer = (workspace.Player.Position - door.Position).Unit
if door.CFrame.LookVector:Dot(toPlayer) > 0 then
	print("The player is in front of the door")
end
```

#### Cross

```luau
Vector3:Cross(other: Vector3): Vector3
```

A vector at right angles to both, as long as the area they span.

#### Lerp

```luau
Vector3:Lerp(goal: Vector3, alpha: number): Vector3
```

The point `alpha` of the way from this vector to `goal` (0 is this one, 1 is `goal`).

```lua
local from = Vector3.new(0, 5, 0)
local to = Vector3.new(20, 5, 0)
workspace.Platform.Position = from:Lerp(to, 0.5) -- halfway
```

#### Angle

```luau
Vector3:Angle(other: Vector3, axis: Vector3?): number
```

The angle to `other` in radians, from 0 to pi. With an `axis`, it's negative when turning to `other` goes clockwise looking down the axis.

#### FuzzyEq

```luau
Vector3:FuzzyEq(other: Vector3, epsilon: number?): boolean
```

Whether each component is within `epsilon` (left out: 0.00001) of `other`'s.

#### Min

```luau
Vector3:Min(...: Vector3): Vector3
```

The smallest of each component of this and the other vectors.

#### Max

```luau
Vector3:Max(...: Vector3): Vector3
```

The largest of each component of this and the other vectors.

#### Abs

```luau
Vector3:Abs(): Vector3
```

Each component without its sign.

#### Floor

```luau
Vector3:Floor(): Vector3
```

Each component rounded down.

#### Ceil

```luau
Vector3:Ceil(): Vector3
```

Each component rounded up.

#### new

```luau
Vector3.new(x: number?, y: number?, z: number?): Vector3
```

A new Vector3. Left-out components are 0.

```lua
local a = Vector3.new(1, 2, 3)
local b = Vector3.new(4, 0, 0)
print(a + b, a.Magnitude, (b - a).Unit)
```

