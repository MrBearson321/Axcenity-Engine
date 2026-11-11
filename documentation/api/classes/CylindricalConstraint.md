---
description: "Lets Attachment1's part slide along and turn around Attachment0's X axis, like a drill or a screw."
---

# CylindricalConstraint

```luau
Instance.new("CylindricalConstraint")
```

Lets Attachment1's part slide along and turn around Attachment0's X axis, like a drill or a screw. It has a prismatic constraint's limits and actuator for sliding and a hinge's (AngularLimitsEnabled, AngularActuatorType) for turning. `CurrentPosition` and `CurrentAngle` are read-only.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
CylindricalConstraint.Attachment0: Instance?
```

The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).

#### Attachment1

```luau
CylindricalConstraint.Attachment1: Instance?
```

The Attachment on the second part, held to Attachment0.

#### Enabled

```luau
CylindricalConstraint.Enabled: boolean
```

On (the default), it joins the attachments' parts. Off, it holds nothing.

#### Visible

```luau
CylindricalConstraint.Visible: boolean
```

On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.

#### Color

```luau
CylindricalConstraint.Color: Color3
```

The color it's drawn in while Visible.

#### BreakForce

```luau
CylindricalConstraint.BreakForce: number
```

How much force (mass times studs a second squared: a 1×1×1 plastic block weighs 196.2) holds it together before it breaks, turns off and fires Broken. 0 (the default) never breaks. A weld with a BreakForce or BreakTorque holds its parts as two bodies joined by a joint, so its parts aren't one assembly.

#### BreakTorque

```luau
CylindricalConstraint.BreakTorque: number
```

How much torque holds it together before it breaks, turns off and fires Broken; a weld's is measured halfway between its parts. 0 (the default) never breaks.

#### LimitsEnabled

```luau
CylindricalConstraint.LimitsEnabled: boolean
```

On, Attachment1 only slides between LowerLimit and UpperLimit along the axis.

#### LowerLimit

```luau
CylindricalConstraint.LowerLimit: number
```

While LimitsEnabled is on, the closest Attachment1 can be to Attachment0 along the axis, in studs (negative is behind it).

#### UpperLimit

```luau
CylindricalConstraint.UpperLimit: number
```

While LimitsEnabled is on, the farthest Attachment1 can slide along the axis, in studs.

#### ActuatorType

```luau
CylindricalConstraint.ActuatorType: EnumActuatorType | "None" | "Motor" | "Servo"
```

What slides it: `Enum.ActuatorType.None` (free), `Motor` (slides at Velocity) or `Servo` (slides to TargetPosition and holds it).

#### Velocity

```luau
CylindricalConstraint.Velocity: number
```

With a Motor ActuatorType, how fast it slides Attachment1 along the axis, in studs a second.

#### MotorMaxForce

```luau
CylindricalConstraint.MotorMaxForce: number
```

With a Motor ActuatorType, the most force it slides with. `math.huge` has no limit.

#### TargetPosition

```luau
CylindricalConstraint.TargetPosition: number
```

With a Servo ActuatorType, how far along the axis it slides Attachment1 to and holds, in studs.

#### Speed

```luau
CylindricalConstraint.Speed: number
```

With a Servo ActuatorType, the fastest it slides toward TargetPosition, in studs a second.

#### ServoMaxForce

```luau
CylindricalConstraint.ServoMaxForce: number
```

With a Servo ActuatorType, the most force it slides and holds with.

#### AngularLimitsEnabled

```luau
CylindricalConstraint.AngularLimitsEnabled: boolean
```

On, it only turns between LowerAngle and UpperAngle.

#### LowerAngle

```luau
CylindricalConstraint.LowerAngle: number
```

While AngularLimitsEnabled is on, the farthest it turns one way, in degrees.

#### UpperAngle

```luau
CylindricalConstraint.UpperAngle: number
```

While AngularLimitsEnabled is on, the farthest it turns the other way, in degrees.

#### AngularActuatorType

```luau
CylindricalConstraint.AngularActuatorType: EnumActuatorType | "None" | "Motor" | "Servo"
```

What turns it around the axis: `Enum.ActuatorType.None` (free), `Motor` (spins at AngularVelocity) or `Servo` (turns to TargetAngle).

#### AngularVelocity

```luau
CylindricalConstraint.AngularVelocity: number
```

With a Motor actuator, how fast it turns Attachment1 around the axis, in radians a second.

#### MotorMaxTorque

```luau
CylindricalConstraint.MotorMaxTorque: number
```

With a Motor actuator, the most torque it turns with. `math.huge` has no limit.

#### TargetAngle

```luau
CylindricalConstraint.TargetAngle: number
```

With a Servo actuator, the angle it turns Attachment1 to and holds, in degrees.

#### AngularSpeed

```luau
CylindricalConstraint.AngularSpeed: number
```

With a Servo actuator, the fastest it turns toward TargetAngle, in radians a second.

#### ServoMaxTorque

```luau
CylindricalConstraint.ServoMaxTorque: number
```

With a Servo actuator, the most torque it turns and holds with. Too little, and a heavy load drags it.

#### CurrentAngle

```luau
CylindricalConstraint.CurrentAngle: number
```

How far Attachment1 is turned from Attachment0 around the axis now, in degrees from -180 to 180 (read-only).

#### CurrentPosition

```luau
CylindricalConstraint.CurrentPosition: number
```

How far Attachment1 is from Attachment0 along the axis now, in studs (read-only).

### Events

#### Broken

```luau
CylindricalConstraint.Broken: Signal
```

Fires once when the force or torque holding it together goes past its BreakForce or BreakTorque. It's already off (Enabled is false). The handler gets no arguments.

