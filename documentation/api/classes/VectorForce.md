---
description: "Pushes Attachment0's part with a constant Force every physics step, at Attachment0 or (ApplyAtCenterOfMass) its center of mass."
---

# VectorForce

```luau
Instance.new("VectorForce")
```

Pushes Attachment0's part with a constant Force every physics step, at Attachment0 or (ApplyAtCenterOfMass) its center of mass. A force of its mass times `workspace.Gravity` upward makes it hover.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
VectorForce.Attachment0: Instance?
```

The Attachment it pushes or turns, on the part it moves (with everything welded to it).

#### Attachment1

```luau
VectorForce.Attachment1: Instance?
```

The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.

#### Enabled

```luau
VectorForce.Enabled: boolean
```

On (the default), it acts. Off, it does nothing.

#### Force

```luau
VectorForce.Force: Vector3
```

The force it pushes with every step. Its part's mass times `workspace.Gravity` upward makes the part hover.

#### RelativeTo

```luau
VectorForce.RelativeTo: EnumActuatorRelativeTo | "Attachment0" | "Attachment1" | "World"
```

Which axes its vector is in: `Enum.ActuatorRelativeTo.World`, `Attachment0` (turning with the part) or `Attachment1`. Attachment0 by default.

#### ApplyAtCenterOfMass

```luau
VectorForce.ApplyAtCenterOfMass: boolean
```

On, it pushes at the center of mass, so it never turns the part. Off (the default), it pushes at Attachment0.

