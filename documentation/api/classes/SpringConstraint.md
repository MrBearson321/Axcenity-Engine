---
description: "Pulls or pushes its attachments toward FreeLength apart with Stiffness, slowed by Damping, never shorter than MinLength or longer than MaxLength."
---

# SpringConstraint

```luau
Instance.new("SpringConstraint")
```

Pulls or pushes its attachments toward FreeLength apart with Stiffness, slowed by Damping, never shorter than MinLength or longer than MaxLength. `CurrentLength` (read-only) is how far apart they are. Visible draws a coil.

It also has every member of [Instance](Instance.md).

### Properties

#### Attachment0

```luau
SpringConstraint.Attachment0: Instance?
```

The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).

#### Attachment1

```luau
SpringConstraint.Attachment1: Instance?
```

The Attachment on the second part, held to Attachment0.

#### Enabled

```luau
SpringConstraint.Enabled: boolean
```

On (the default), it joins the attachments' parts. Off, it holds nothing.

#### Visible

```luau
SpringConstraint.Visible: boolean
```

On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.

#### Color

```luau
SpringConstraint.Color: Color3
```

The color it's drawn in while Visible.

#### BreakForce

```luau
SpringConstraint.BreakForce: number
```

How much force (mass times studs a second squared: a 1×1×1 plastic block weighs 196.2) holds it together before it breaks, turns off and fires Broken. 0 (the default) never breaks. A weld with a BreakForce or BreakTorque holds its parts as two bodies joined by a joint, so its parts aren't one assembly.

#### BreakTorque

```luau
SpringConstraint.BreakTorque: number
```

How much torque holds it together before it breaks, turns off and fires Broken; a weld's is measured halfway between its parts. 0 (the default) never breaks.

#### Stiffness

```luau
SpringConstraint.Stiffness: number
```

How hard it pulls or pushes toward FreeLength: force per stud it's stretched or squashed.

#### Damping

```luau
SpringConstraint.Damping: number
```

How much it resists moving, so it stops bouncing: force per stud a second of stretching.

#### FreeLength

```luau
SpringConstraint.FreeLength: number
```

The length it settles at with nothing pulling on it, in studs.

#### MinLength

```luau
SpringConstraint.MinLength: number
```

The shortest it can be squashed, in studs.

#### MaxLength

```luau
SpringConstraint.MaxLength: number
```

The longest it can be stretched, in studs.

#### Coils

```luau
SpringConstraint.Coils: number
```

How many turns the drawn coil has.

#### Radius

```luau
SpringConstraint.Radius: number
```

The drawn coil's radius, in studs.

#### Thickness

```luau
SpringConstraint.Thickness: number
```

How thick it's drawn while Visible, in studs.

#### CurrentLength

```luau
SpringConstraint.CurrentLength: number
```

How far apart the attachments are now, in studs (read-only).

### Events

#### Broken

```luau
SpringConstraint.Broken: Signal
```

Fires once when the force or torque holding it together goes past its BreakForce or BreakTorque. It's already off (Enabled is false). The handler gets no arguments.

