---
description: "A point or size on the screen, in pixels, such as a ScrollingFrame's CanvasPosition."
---

# Vector2

A point or size on the screen, in pixels, such as a ScrollingFrame's CanvasPosition.

### Properties

#### X

```luau
Vector2.X: number
```

Across, in pixels.

#### Y

```luau
Vector2.Y: number
```

Down, in pixels.

#### Magnitude

```luau
Vector2.Magnitude: number
```

The length of the vector.

#### zero

```luau
Vector2.zero: Vector2
```

(0, 0).

#### one

```luau
Vector2.one: Vector2
```

(1, 1).

#### xAxis

```luau
Vector2.xAxis: Vector2
```

(1, 0).

#### yAxis

```luau
Vector2.yAxis: Vector2
```

(0, 1).

### Functions

#### Dot

```luau
Vector2:Dot(other: Vector2): number
```

The dot product: the lengths multiplied, times the cosine of the angle between them.

#### Lerp

```luau
Vector2:Lerp(goal: Vector2, alpha: number): Vector2
```

The point `alpha` of the way from this vector to `goal` (0 is this one, 1 is `goal`).

#### Angle

```luau
Vector2:Angle(other: Vector2, isSigned: boolean?): number
```

The angle to `other` in radians, from 0 to pi. Signed, it's negative when `other` is anticlockwise from this one on the screen.

#### FuzzyEq

```luau
Vector2:FuzzyEq(other: Vector2, epsilon: number?): boolean
```

Whether each component is within `epsilon` (left out: 0.00001) of `other`'s.

#### Min

```luau
Vector2:Min(...: Vector2): Vector2
```

The smallest of each component of this and the other vectors.

#### Max

```luau
Vector2:Max(...: Vector2): Vector2
```

The largest of each component of this and the other vectors.

#### Abs

```luau
Vector2:Abs(): Vector2
```

Each component without its sign.

#### Floor

```luau
Vector2:Floor(): Vector2
```

Each component rounded down.

#### Ceil

```luau
Vector2:Ceil(): Vector2
```

Each component rounded up.

#### new

```luau
Vector2.new(x: number?, y: number?): Vector2
```

A new Vector2. Left-out components are 0.

```lua
local center = Vector2.new(400, 300)
print(center.X, center.Y)
```

