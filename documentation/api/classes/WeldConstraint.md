---
description: "Holds Part0 and Part1 together where they were when it was made (or enabled)."
---

# WeldConstraint

```luau
Instance.new("WeldConstraint")
```

Holds `Part0` and `Part1` together where they were when it was made (or enabled). Parts welded together move as one body, an assembly: they fall and tumble together, and an anchored part anchors the whole assembly. `part:GetConnectedParts()` and `part:GetRootPart()` follow welds. Setting a welded part's Position or CFrame moves that part alone, and the weld holds it where it lands.

It also has every member of [Instance](Instance.md).

### Properties

#### Part0

```luau
WeldConstraint.Part0: Instance?
```

One of the two parts it holds together.

#### Part1

```luau
WeldConstraint.Part1: Instance?
```

The other part it holds together.

#### Enabled

```luau
WeldConstraint.Enabled: boolean
```

On (the default), it holds the parts together. Off, they move apart; turned on again, it welds them where they are then.

#### BreakForce

```luau
WeldConstraint.BreakForce: number
```

How much force (mass times studs a second squared: a 1×1×1 plastic block weighs 196.2) holds it together before it breaks, turns off and fires Broken. 0 (the default) never breaks. A weld with a BreakForce or BreakTorque holds its parts as two bodies joined by a joint, so its parts aren't one assembly.

#### BreakTorque

```luau
WeldConstraint.BreakTorque: number
```

How much torque holds it together before it breaks, turns off and fires Broken; a weld's is measured halfway between its parts. 0 (the default) never breaks.

### Events

#### Broken

```luau
WeldConstraint.Broken: Signal
```

Fires once when the force or torque holding it together goes past its BreakForce or BreakTorque. It's already off (Enabled is false). The handler gets no arguments.

