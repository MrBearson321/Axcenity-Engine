---
description: "One point of a NumberSequence."
---

# NumberSequenceKeypoint

One point of a NumberSequence.

### Properties

#### Time

```luau
NumberSequenceKeypoint.Time: number
```

When, from 0 to 1.

#### Value

```luau
NumberSequenceKeypoint.Value: number
```

The number then.

#### Envelope

```luau
NumberSequenceKeypoint.Envelope: number
```

Give or take: each particle picks once where in this range it is.

### Functions

#### new

```luau
NumberSequenceKeypoint.new(time: number, value: number, envelope: number?): NumberSequenceKeypoint
```

A point at `time` (0 to 1) with `value`, give or take `envelope` (0 when left out).

