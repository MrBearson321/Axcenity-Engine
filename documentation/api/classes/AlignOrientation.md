---
description: "Turns Attachment0's part so Attachment0 faces the way Attachment1 does (TwoAttachment Mode) or CFrame's rotation (OneAttachment), smoothly, in about 1/Responsiveness seconds, with at most MaxTorque and MaxAngularVelocity."
---

# AlignOrientation

```luau
Instance.new("AlignOrientation")
```

Turns Attachment0's part so Attachment0 faces the way Attachment1 does (TwoAttachment Mode) or CFrame's rotation (OneAttachment), smoothly, in about 1/Responsiveness seconds, with at most MaxTorque and MaxAngularVelocity. A level CFrame keeps a drone level.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
AlignOrientation.Attachment0: Instance?
```

The Attachment it pushes or turns, on the part it moves (with everything welded to it).

#### Attachment1

```luau
AlignOrientation.Attachment1: Instance?
```

The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.

#### Enabled

```luau
AlignOrientation.Enabled: boolean
```

On (the default), it acts. Off, it does nothing.

#### Mode

```luau
AlignOrientation.Mode: EnumOrientationAlignmentMode | "OneAttachment" | "TwoAttachment"
```

`Enum.OrientationAlignmentMode.TwoAttachment` (the default) turns Attachment0 to face the way Attachment1 does; `OneAttachment` turns it to CFrame's rotation.

#### CFrame

```luau
AlignOrientation.CFrame: CFrame
```

The rotation it turns Attachment0 to in OneAttachment Mode, in the world. Its position is ignored.

#### MaxTorque

```luau
AlignOrientation.MaxTorque: number
```

The most torque it turns with. `math.huge` has no limit.

#### MaxAngularVelocity

```luau
AlignOrientation.MaxAngularVelocity: number
```

The fastest it turns Attachment0 toward its target, in radians a second.

#### Responsiveness

```luau
AlignOrientation.Responsiveness: number
```

How quickly it gets there, from 5 to 200: it closes most of the turn in about 1/Responsiveness seconds, smoothly, without overshooting.

#### RigidityEnabled

```luau
AlignOrientation.RigidityEnabled: boolean
```

On, it turns there as fast as MaxTorque and MaxAngularVelocity allow, ignoring Responsiveness.

