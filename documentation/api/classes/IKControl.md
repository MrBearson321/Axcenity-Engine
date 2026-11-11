---
description: "Inverse kinematics during the game, for the skinned MeshPart it's in (or its Animator)."
---

# IKControl

```luau
Instance.new("IKControl")
```

Inverse kinematics during the game, for the skinned MeshPart it's in (or its Animator). After the Animator and before drawing, it turns the bones from ChainRoot down to EndEffector toward its Target, by its Type. Weight and Enabled fade in and out over SmoothTime.

It also has every member of [Instance](Instance.md).

### Properties

#### Type

```luau
IKControl.Type: string
```

How it turns the bones: `"Position"` puts EndEffector at the Target, `"Rotation"` turns it as the Target is turned from the MeshPart, `"Transform"` does both, and `"LookAt"` faces it at the Target.

#### Enabled

```luau
IKControl.Enabled: boolean
```

If false, the pose goes back to the plain animation over SmoothTime.

#### EndEffector

```luau
IKControl.EndEffector: string
```

The bone that reaches or faces the Target, by its name or path, such as `"Head"` (which finds `mixamorig:Head`).

#### ChainRoot

```luau
IKControl.ChainRoot: string
```

The highest bone that turns. Empty is the two bones above EndEffector for Position and Transform, or just EndEffector for LookAt and Rotation.

#### Target

```luau
IKControl.Target: Instance?
```

The Part or Attachment to reach or face. With none, the pose goes back to the plain animation over SmoothTime.

#### Pole

```luau
IKControl.Pole: Instance?
```

A Part or Attachment that the elbow or knee of a Position or Transform chain bends toward. With none, the limb bends the way it already bends.

#### Weight

```luau
IKControl.Weight: number
```

How much of the solved pose is used, from 0 (the plain animation) to 1. A change fades in over SmoothTime.

#### SmoothTime

```luau
IKControl.SmoothTime: number
```

Seconds a change in Weight or Enabled takes, and about how long the IK trails its Target. 0 follows at once.

#### MaxAngle

```luau
IKControl.MaxAngle: number
```

LookAt only: the most degrees the EndEffector turns from the way the character faces.

#### Forward

```luau
IKControl.Forward: Vector3
```

LookAt only: the way the EndEffector faces in the MeshPart's own axes, such as `Vector3.new(0, 0, 1)`. (0, 0, 0) works it out from the way the skeleton's toes point, or else -Z.

