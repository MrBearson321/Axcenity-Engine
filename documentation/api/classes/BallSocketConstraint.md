---
description: "Holds two attachments together and lets them turn any way, like a shoulder or a pendulum."
---

# BallSocketConstraint

```luau
Instance.new("BallSocketConstraint")
```

Holds two attachments together and lets them turn any way, like a shoulder or a pendulum. LimitsEnabled keeps Attachment1's axis within UpperAngle of Attachment0's; TwistLimitsEnabled limits twisting around it.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
BallSocketConstraint.Attachment0: Instance?
```

The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).

#### Attachment1

```luau
BallSocketConstraint.Attachment1: Instance?
```

The Attachment on the second part, held to Attachment0.

#### Enabled

```luau
BallSocketConstraint.Enabled: boolean
```

On (the default), it joins the attachments' parts. Off, it holds nothing.

#### Visible

```luau
BallSocketConstraint.Visible: boolean
```

On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.

#### Color

```luau
BallSocketConstraint.Color: Color3
```

The color it's drawn in while Visible.

#### BreakForce

```luau
BallSocketConstraint.BreakForce: number
```

How much force (mass times studs a second squared: a 1×1×1 plastic block weighs 196.2) holds it together before it breaks, turns off and fires Broken. 0 (the default) never breaks. A weld with a BreakForce or BreakTorque holds its parts as two bodies joined by a joint, so its parts aren't one assembly.

#### BreakTorque

```luau
BallSocketConstraint.BreakTorque: number
```

How much torque holds it together before it breaks, turns off and fires Broken; a weld's is measured halfway between its parts. 0 (the default) never breaks.

#### LimitsEnabled

```luau
BallSocketConstraint.LimitsEnabled: boolean
```

On, Attachment1's axis can only lean UpperAngle from Attachment0's.

#### UpperAngle

```luau
BallSocketConstraint.UpperAngle: number
```

While LimitsEnabled is on, how far Attachment1's axis can lean from Attachment0's, in degrees.

#### TwistLimitsEnabled

```luau
BallSocketConstraint.TwistLimitsEnabled: boolean
```

On, Attachment1 can only twist around its axis between TwistLowerAngle and TwistUpperAngle.

#### TwistLowerAngle

```luau
BallSocketConstraint.TwistLowerAngle: number
```

While TwistLimitsEnabled is on, the farthest it twists one way, in degrees.

#### TwistUpperAngle

```luau
BallSocketConstraint.TwistUpperAngle: number
```

While TwistLimitsEnabled is on, the farthest it twists the other way, in degrees.

### Events

#### Broken

```luau
BallSocketConstraint.Broken: Signal
```

Fires once when the force or torque holding it together goes past its BreakForce or BreakTorque. It's already off (Enabled is false). The handler gets no arguments.

