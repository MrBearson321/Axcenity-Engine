---
description: "A curve through its SplinePoint children, in order (and back to the first when Closed), for SplineFollowers to move parts along, NavigationAgents to patrol and the Sequencer's camera to ride."
---

# Spline

```luau
Instance.new("Spline")
```

A curve through its SplinePoint children, in order (and back to the first when Closed), for SplineFollowers to move parts along, NavigationAgents to patrol and the Sequencer's camera to ride. Measure it with Length, GetPositionAt, GetCFrameAt and GetClosestDistance. Drawn in the editor, never in the game: click it with Select to edit its points.

It also has every member of [Instance](Instance.md).

### Properties

#### Closed

```luau
Spline.Closed: boolean
```

If true, the curve goes on from the last SplinePoint back to the first, as a loop.

#### Length

```luau
Spline.Length: number
```

How long its curve is, in studs (read-only; 0 with fewer than two points).

### Functions

#### GetPositionAt

```luau
Spline:GetPositionAt(t: number): Vector3
```

The point `t` of the way along the curve by distance: 0 is its first point, 1 its end (its first point again when Closed), 0.5 halfway along. Errors if it has no SplinePoint.

#### GetCFrameAt

```luau
Spline:GetCFrameAt(distance: number): CFrame
```

Where the curve is `distance` studs along it, facing the way it goes (upright, never rolled). A distance past an end is wrapped round a Closed spline and held at the end of an open one. Errors if it has no SplinePoint.

#### GetClosestDistance

```luau
Spline:GetClosestDistance(position: Vector3): number
```

How far along the curve its point nearest `position` is, in studs: pass it to GetCFrameAt. Errors if it has no SplinePoint.

