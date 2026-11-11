---
description: "Moves the Part (with the parts welded to it) or the Model it's parented to along its Spline at Speed during Play: moving platforms, trains and gondolas."
---

# SplineFollower

```luau
Instance.new("SplineFollower")
```

Moves the Part (with the parts welded to it) or the Model it's parented to along its Spline at Speed during Play: moving platforms, trains and gondolas. It starts at the point of the curve nearest where its parent is, and Loops, PingPongs or stops Once at the end, firing Reached. Characters standing on what it moves ride along.

It also has every member of [Instance](Instance.md).

### Properties

#### Spline

```luau
SplineFollower.Spline: Instance?
```

The Spline it moves its parent along. nil moves nothing.

#### Speed

```luau
SplineFollower.Speed: number
```

Studs a second along the curve, the same speed however its points are spaced.

#### Mode

```luau
SplineFollower.Mode: EnumSplineFollowMode | "Loop" | "PingPong" | "Once"
```

What it does at the ends: `Enum.SplineFollowMode.Loop` goes on from the start again (round a Closed spline), `PingPong` (the default) turns back at each end, and `Once` stops at the end.

#### FaceAlong

```luau
SplineFollower.FaceAlong: boolean
```

If true, its parent turns to face the way it goes along the curve, upright, as a train does. If false (the default), it keeps the way it was turned.

#### Enabled

```luau
SplineFollower.Enabled: boolean
```

If false, it stops where it is; true carries on from there.

### Events

#### Reached

```luau
SplineFollower.Reached: Signal
```

Fires as it gets to an end of its spline, with "End" or "Start": a Loop as it comes round to the start again, a PingPong as it turns back, and a Once as it stops.

