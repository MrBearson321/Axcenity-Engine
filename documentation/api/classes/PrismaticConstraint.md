---
description: "Lets Attachment1's part slide along Attachment0's X axis only, like a drawer or a piston."
---

# PrismaticConstraint

```luau
Instance.new("PrismaticConstraint")
```

Lets Attachment1's part slide along Attachment0's X axis only, like a drawer or a piston. LimitsEnabled keeps it between LowerLimit and UpperLimit; ActuatorType Motor slides it at Velocity, Servo to TargetPosition. `CurrentPosition` (read-only) is how far along it is, in studs.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
PrismaticConstraint.Attachment0: Instance?
```

The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).

#### Attachment1

```luau
PrismaticConstraint.Attachment1: Instance?
```

The Attachment on the second part, held to Attachment0.

#### Enabled

```luau
PrismaticConstraint.Enabled: boolean
```

On (the default), it joins the attachments' parts. Off, it holds nothing.

#### Visible

```luau
PrismaticConstraint.Visible: boolean
```

On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.

#### Color

```luau
PrismaticConstraint.Color: Color3
```

The color it's drawn in while Visible.

#### BreakForce

```luau
PrismaticConstraint.BreakForce: number
```

How much force (mass times studs a second squared: a 1×1×1 plastic block weighs 196.2) holds it together before it breaks, turns off and fires Broken. 0 (the default) never breaks. A weld with a BreakForce or BreakTorque holds its parts as two bodies joined by a joint, so its parts aren't one assembly.

#### BreakTorque

```luau
PrismaticConstraint.BreakTorque: number
```

How much torque holds it together before it breaks, turns off and fires Broken; a weld's is measured halfway between its parts. 0 (the default) never breaks.

#### LimitsEnabled

```luau
PrismaticConstraint.LimitsEnabled: boolean
```

On, Attachment1 only slides between LowerLimit and UpperLimit along the axis.

#### LowerLimit

```luau
PrismaticConstraint.LowerLimit: number
```

While LimitsEnabled is on, the closest Attachment1 can be to Attachment0 along the axis, in studs (negative is behind it).

#### UpperLimit

```luau
PrismaticConstraint.UpperLimit: number
```

While LimitsEnabled is on, the farthest Attachment1 can slide along the axis, in studs.

#### ActuatorType

```luau
PrismaticConstraint.ActuatorType: EnumActuatorType | "None" | "Motor" | "Servo"
```

What slides it: `Enum.ActuatorType.None` (free), `Motor` (slides at Velocity) or `Servo` (slides to TargetPosition and holds it).

#### Velocity

```luau
PrismaticConstraint.Velocity: number
```

With a Motor ActuatorType, how fast it slides Attachment1 along the axis, in studs a second.

#### MotorMaxForce

```luau
PrismaticConstraint.MotorMaxForce: number
```

With a Motor ActuatorType, the most force it slides with. `math.huge` has no limit.

#### TargetPosition

```luau
PrismaticConstraint.TargetPosition: number
```

With a Servo ActuatorType, how far along the axis it slides Attachment1 to and holds, in studs.

#### Speed

```luau
PrismaticConstraint.Speed: number
```

With a Servo ActuatorType, the fastest it slides toward TargetPosition, in studs a second.

#### ServoMaxForce

```luau
PrismaticConstraint.ServoMaxForce: number
```

With a Servo ActuatorType, the most force it slides and holds with.

#### CurrentPosition

```luau
PrismaticConstraint.CurrentPosition: number
```

How far Attachment1 is from Attachment0 along the axis now, in studs (read-only).

### Events

#### Broken

```luau
PrismaticConstraint.Broken: Signal
```

Fires once when the force or torque holding it together goes past its BreakForce or BreakTorque. It's already off (Enabled is false). The handler gets no arguments.

