---
description: "Keeps Attachment0's part (and everything welded to it) moving at VectorVelocity, pushing with at most MaxForce through its center of mass."
---

# LinearVelocity

```luau
Instance.new("LinearVelocity")
```

Keeps Attachment0's part (and everything welded to it) moving at VectorVelocity, pushing with at most MaxForce through its center of mass. It holds the part up against gravity too, so a zero VectorVelocity holds it still in the air.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
LinearVelocity.Attachment0: Instance?
```

The Attachment it pushes or turns, on the part it moves (with everything welded to it).

#### Attachment1

```luau
LinearVelocity.Attachment1: Instance?
```

The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.

#### Enabled

```luau
LinearVelocity.Enabled: boolean
```

On (the default), it acts. Off, it does nothing.

#### VectorVelocity

```luau
LinearVelocity.VectorVelocity: Vector3
```

The velocity it keeps Attachment0's part at, in studs a second, in RelativeTo's axes. Zero holds the part still in the air.

#### MaxForce

```luau
LinearVelocity.MaxForce: number
```

The most force it pushes with, gravity included. A part too heavy for it sinks; `math.huge` has no limit.

#### RelativeTo

```luau
LinearVelocity.RelativeTo: EnumActuatorRelativeTo | "Attachment0" | "Attachment1" | "World"
```

Which axes its vector is in: `Enum.ActuatorRelativeTo.World`, `Attachment0` (turning with the part) or `Attachment1`. World by default.

