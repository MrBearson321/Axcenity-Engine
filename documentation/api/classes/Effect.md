---
description: "A change to the Model it's in: to an attribute, or a Health or Humanoid property."
---

# Effect

```luau
Instance.new("Effect")
```

A change to the Model it's in: to an attribute, or a Health or Humanoid property. It's instant, lasts Duration seconds, or repeats every Period (poison, a heal over time); it adds or multiplies, stacks to MaxStacks, and gives GrantsTags while active. Removing it undoes a lasting change. Keep templates out of Models (in ReplicatedStorage, ServerStorage or an Ability) and put copies on with `model:ApplyEffect(effect)`.

It also has every member of [Instance](Instance.md).

### Properties

#### Attribute

```luau
Effect.Attribute: string
```

What it changes on the Model it's in: a property of its Health (`Health`, `MaxHealth`, `RegenRate`), of its character controller, as the Humanoid's (`WalkSpeed`, `JumpHeight`), or else the Model's number attribute of this name.

#### Operation

```luau
Effect.Operation: EnumEffectOperation | "Add" | "Multiply"
```

`Enum.EffectOperation.Add` adds Amount (a negative Amount on Health is damage); `Multiply` multiplies by it. Each stack does it again.

#### Amount

```luau
Effect.Amount: number
```

How much it adds, or what it multiplies by.

#### Duration

```luau
Effect.Duration: number
```

Seconds it lasts. 0 is instant: it changes once and is gone. Below 0 it lasts until it's removed.

#### Period

```luau
Effect.Period: number
```

With a Duration: above 0, it changes again every Period seconds, for good (poison, a heal over time). 0 changes once for as long as it lasts, and removing it undoes that.

#### MaxStacks

```luau
Effect.MaxStacks: number
```

How many times applying it to a Model that has it already adds to it. Applying it again also starts its Duration over.

#### GrantsTags

```luau
Effect.GrantsTags: string
```

Tags, separated by commas, its Model has while it's active.

#### Stacks

```luau
Effect.Stacks: number
```

Read-only: how many times it's applied.

#### TimeLeft

```luau
Effect.TimeLeft: number
```

Read-only: the seconds it has left.

