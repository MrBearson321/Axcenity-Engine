---
description: "Turns Attachment0's part with a constant Torque every physics step."
---

# Torque

```luau
Instance.new("Torque")
```

Turns Attachment0's part with a constant Torque every physics step.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
Torque.Attachment0: Instance?
```

The Attachment it pushes or turns, on the part it moves (with everything welded to it).

#### Attachment1

```luau
Torque.Attachment1: Instance?
```

The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.

#### Enabled

```luau
Torque.Enabled: boolean
```

On (the default), it acts. Off, it does nothing.

#### Torque

```luau
Torque.Torque: Vector3
```

The torque it turns its part with every step, around RelativeTo's axes.

#### RelativeTo

```luau
Torque.RelativeTo: EnumActuatorRelativeTo | "Attachment0" | "Attachment1" | "World"
```

Which axes its vector is in: `Enum.ActuatorRelativeTo.World`, `Attachment0` (turning with the part) or `Attachment1`. Attachment0 by default.

