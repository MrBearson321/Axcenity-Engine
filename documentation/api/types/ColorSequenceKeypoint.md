---
description: "One point of a ColorSequence."
---

# ColorSequenceKeypoint

One point of a ColorSequence.

### Properties

#### Time

```luau
ColorSequenceKeypoint.Time: number
```

When, from 0 to 1.

#### Value

```luau
ColorSequenceKeypoint.Value: Color3
```

The color then.

### Functions

#### new

```luau
ColorSequenceKeypoint.new(time: number, color: Color3): ColorSequenceKeypoint
```

A point at `time` (0 to 1) with `color`.

