---
description: "Keeps its attachments exactly Length apart, letting them turn freely."
---

# RodConstraint

```luau
Instance.new("RodConstraint")
```

Keeps its attachments exactly Length apart, letting them turn freely. `CurrentLength` (read-only) is how far apart they are.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
RodConstraint.Attachment0: Instance?
```

The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).

#### Attachment1

```luau
RodConstraint.Attachment1: Instance?
```

The Attachment on the second part, held to Attachment0.

#### Enabled

```luau
RodConstraint.Enabled: boolean
```

On (the default), it joins the attachments' parts. Off, it holds nothing.

#### Visible

```luau
RodConstraint.Visible: boolean
```

On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.

#### Color

```luau
RodConstraint.Color: Color3
```

The color it's drawn in while Visible.

#### BreakForce

```luau
RodConstraint.BreakForce: number
```

How much force (mass times studs a second squared: a 1×1×1 plastic block weighs 196.2) holds it together before it breaks, turns off and fires Broken. 0 (the default) never breaks. A weld with a BreakForce or BreakTorque holds its parts as two bodies joined by a joint, so its parts aren't one assembly.

#### BreakTorque

```luau
RodConstraint.BreakTorque: number
```

How much torque holds it together before it breaks, turns off and fires Broken; a weld's is measured halfway between its parts. 0 (the default) never breaks.

#### Length

```luau
RodConstraint.Length: number
```

How far apart the attachments are held, in studs.

#### Thickness

```luau
RodConstraint.Thickness: number
```

How thick it's drawn while Visible, in studs.

#### CurrentLength

```luau
RodConstraint.CurrentLength: number
```

How far apart the attachments are now, in studs (read-only).

### Events

#### Broken

```luau
RodConstraint.Broken: Signal
```

Fires once when the force or torque holding it together goes past its BreakForce or BreakTorque. It's already off (Enabled is false). The handler gets no arguments.

