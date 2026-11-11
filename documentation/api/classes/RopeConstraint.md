---
description: "Keeps its attachments at most Length apart; closer, it's slack."
---

# RopeConstraint

```luau
Instance.new("RopeConstraint")
```

Keeps its attachments at most Length apart; closer, it's slack. With WinchEnabled, a winch reels it toward WinchTarget at WinchSpeed, lifting loads up to WinchForce. `CurrentLength` (read-only) is how far apart they are.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
RopeConstraint.Attachment0: Instance?
```

The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).

#### Attachment1

```luau
RopeConstraint.Attachment1: Instance?
```

The Attachment on the second part, held to Attachment0.

#### Enabled

```luau
RopeConstraint.Enabled: boolean
```

On (the default), it joins the attachments' parts. Off, it holds nothing.

#### Visible

```luau
RopeConstraint.Visible: boolean
```

On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.

#### Color

```luau
RopeConstraint.Color: Color3
```

The color it's drawn in while Visible.

#### BreakForce

```luau
RopeConstraint.BreakForce: number
```

How much force (mass times studs a second squared: a 1×1×1 plastic block weighs 196.2) holds it together before it breaks, turns off and fires Broken. 0 (the default) never breaks. A weld with a BreakForce or BreakTorque holds its parts as two bodies joined by a joint, so its parts aren't one assembly.

#### BreakTorque

```luau
RopeConstraint.BreakTorque: number
```

How much torque holds it together before it breaks, turns off and fires Broken; a weld's is measured halfway between its parts. 0 (the default) never breaks.

#### Length

```luau
RopeConstraint.Length: number
```

The farthest apart the attachments can be, in studs. Closer, the rope is slack.

#### WinchEnabled

```luau
RopeConstraint.WinchEnabled: boolean
```

On, a winch reels the rope in or out toward WinchTarget, starting from Length.

#### WinchTarget

```luau
RopeConstraint.WinchTarget: number
```

The length the winch reels the rope to, in studs.

#### WinchSpeed

```luau
RopeConstraint.WinchSpeed: number
```

How fast the winch reels, in studs a second.

#### WinchForce

```luau
RopeConstraint.WinchForce: number
```

The most the winch pulls with: a heavier load stops it reeling in (it still holds).

#### Thickness

```luau
RopeConstraint.Thickness: number
```

How thick it's drawn while Visible, in studs.

#### CurrentLength

```luau
RopeConstraint.CurrentLength: number
```

How far apart the attachments are now, in studs (read-only).

### Events

#### Broken

```luau
RopeConstraint.Broken: Signal
```

Fires once when the force or torque holding it together goes past its BreakForce or BreakTorque. It's already off (Enabled is false). The handler gets no arguments.

