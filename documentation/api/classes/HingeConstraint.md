---
description: "Joins two parts at their attachments so Attachment1's part turns around Attachment0's X axis (its Axis), like a door or a wheel."
---

# HingeConstraint

```luau
Instance.new("HingeConstraint")
```

Joins two parts at their attachments so Attachment1's part turns around Attachment0's X axis (its `Axis`), like a door or a wheel. LimitsEnabled keeps it between LowerAngle and UpperAngle; ActuatorType Motor spins it at AngularVelocity, Servo turns it to TargetAngle and holds it. `CurrentAngle` (read-only) is how far it's turned, in degrees.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
HingeConstraint.Attachment0: Instance?
```

The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).

#### Attachment1

```luau
HingeConstraint.Attachment1: Instance?
```

The Attachment on the second part, held to Attachment0.

#### Enabled

```luau
HingeConstraint.Enabled: boolean
```

On (the default), it joins the attachments' parts. Off, it holds nothing.

#### Visible

```luau
HingeConstraint.Visible: boolean
```

On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.

#### Color

```luau
HingeConstraint.Color: Color3
```

The color it's drawn in while Visible.

#### BreakForce

```luau
HingeConstraint.BreakForce: number
```

How much force (mass times studs a second squared: a 1×1×1 plastic block weighs 196.2) holds it together before it breaks, turns off and fires Broken. 0 (the default) never breaks. A weld with a BreakForce or BreakTorque holds its parts as two bodies joined by a joint, so its parts aren't one assembly.

#### BreakTorque

```luau
HingeConstraint.BreakTorque: number
```

How much torque holds it together before it breaks, turns off and fires Broken; a weld's is measured halfway between its parts. 0 (the default) never breaks.

#### LimitsEnabled

```luau
HingeConstraint.LimitsEnabled: boolean
```

On, Attachment1 only turns between LowerAngle and UpperAngle.

#### LowerAngle

```luau
HingeConstraint.LowerAngle: number
```

While LimitsEnabled is on, the farthest it turns one way, in degrees.

#### UpperAngle

```luau
HingeConstraint.UpperAngle: number
```

While LimitsEnabled is on, the farthest it turns the other way, in degrees.

#### ActuatorType

```luau
HingeConstraint.ActuatorType: EnumActuatorType | "None" | "Motor" | "Servo"
```

What turns it: `Enum.ActuatorType.None` (free), `Motor` (spins at AngularVelocity) or `Servo` (turns to TargetAngle and holds it).

#### AngularVelocity

```luau
HingeConstraint.AngularVelocity: number
```

With a Motor actuator, how fast it turns Attachment1 around the axis, in radians a second.

#### MotorMaxTorque

```luau
HingeConstraint.MotorMaxTorque: number
```

With a Motor actuator, the most torque it turns with. `math.huge` has no limit.

#### TargetAngle

```luau
HingeConstraint.TargetAngle: number
```

With a Servo actuator, the angle it turns Attachment1 to and holds, in degrees.

#### AngularSpeed

```luau
HingeConstraint.AngularSpeed: number
```

With a Servo actuator, the fastest it turns toward TargetAngle, in radians a second.

#### ServoMaxTorque

```luau
HingeConstraint.ServoMaxTorque: number
```

With a Servo actuator, the most torque it turns and holds with. Too little, and a heavy load drags it.

#### CurrentAngle

```luau
HingeConstraint.CurrentAngle: number
```

How far Attachment1 is turned from Attachment0 around the axis now, in degrees from -180 to 180 (read-only).

### Events

#### Broken

```luau
HingeConstraint.Broken: Signal
```

Fires once when the force or torque holding it together goes past its BreakForce or BreakTorque. It's already off (Enabled is false). The handler gets no arguments.

