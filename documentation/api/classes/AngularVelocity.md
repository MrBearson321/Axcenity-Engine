---
description: "Keeps Attachment0's part spinning at AngularVelocity (radians a second), turning it with at most MaxTorque."
---

# AngularVelocity

```luau
Instance.new("AngularVelocity")
```

Keeps Attachment0's part spinning at AngularVelocity (radians a second), turning it with at most MaxTorque.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
AngularVelocity.Attachment0: Instance?
```

The Attachment it pushes or turns, on the part it moves (with everything welded to it).

#### Attachment1

```luau
AngularVelocity.Attachment1: Instance?
```

The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.

#### Enabled

```luau
AngularVelocity.Enabled: boolean
```

On (the default), it acts. Off, it does nothing.

#### AngularVelocity

```luau
AngularVelocity.AngularVelocity: Vector3
```

How fast it spins Attachment0's part around each axis, in radians a second, in RelativeTo's axes.

#### MaxTorque

```luau
AngularVelocity.MaxTorque: number
```

The most torque it turns with. `math.huge` has no limit.

#### RelativeTo

```luau
AngularVelocity.RelativeTo: EnumActuatorRelativeTo | "Attachment0" | "Attachment1" | "World"
```

Which axes its vector is in: `Enum.ActuatorRelativeTo.World`, `Attachment0` (turning with the part) or `Attachment1`. World by default.

