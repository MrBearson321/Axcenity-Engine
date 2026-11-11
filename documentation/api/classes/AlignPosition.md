---
description: "Moves Attachment0's part so Attachment0 reaches Attachment1 (TwoAttachment Mode) or Position (OneAttachment), smoothly, closing most of the gap in about 1/Responsiveness seconds, with at most MaxForce and MaxVelocity."
---

# AlignPosition

```luau
Instance.new("AlignPosition")
```

Moves Attachment0's part so Attachment0 reaches Attachment1 (TwoAttachment Mode) or Position (OneAttachment), smoothly, closing most of the gap in about 1/Responsiveness seconds, with at most MaxForce and MaxVelocity. It holds the part up against gravity.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
AlignPosition.Attachment0: Instance?
```

The Attachment it pushes or turns, on the part it moves (with everything welded to it).

#### Attachment1

```luau
AlignPosition.Attachment1: Instance?
```

The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.

#### Enabled

```luau
AlignPosition.Enabled: boolean
```

On (the default), it acts. Off, it does nothing.

#### Mode

```luau
AlignPosition.Mode: EnumPositionAlignmentMode | "OneAttachment" | "TwoAttachment"
```

`Enum.PositionAlignmentMode.TwoAttachment` (the default) moves Attachment0 to Attachment1, following it; `OneAttachment` moves it to Position.

#### Position

```luau
AlignPosition.Position: Vector3
```

Where it moves Attachment0 in OneAttachment Mode, in the world.

#### MaxForce

```luau
AlignPosition.MaxForce: number
```

The most force it pushes with, gravity included. `math.huge` has no limit.

#### MaxVelocity

```luau
AlignPosition.MaxVelocity: number
```

The fastest it moves Attachment0 toward its target, in studs a second.

#### Responsiveness

```luau
AlignPosition.Responsiveness: number
```

How quickly it gets there, from 5 to 200: it closes most of the gap in about 1/Responsiveness seconds, smoothly, without overshooting.

#### RigidityEnabled

```luau
AlignPosition.RigidityEnabled: boolean
```

On, it gets there as fast as MaxForce and MaxVelocity allow, ignoring Responsiveness.

