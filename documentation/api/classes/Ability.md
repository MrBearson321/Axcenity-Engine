---
description: "Something a character can do, in its Model or its Player: a Fireball, a dash, a heal."
---

# Ability

```luau
Instance.new("Ability")
```

Something a character can do, in its Model or its Player: a Fireball, a dash, a heal. Activating it (its ActivationKey, or Activate) spends CostAmount of its holder's CostAttribute and starts its Cooldown, unless its holder has one of BlockedByTags; while active it gives GrantsTags. It only activates on the server, which checks again whatever a player's device says. The ability bar shows the player's abilities with their cooldowns.

It also has every member of [Instance](Instance.md).

### Properties

#### Cooldown

```luau
Ability.Cooldown: number
```

Seconds after it activates before it can activate again.

#### CostAttribute

```luau
Ability.CostAttribute: string
```

The number attribute activating spends, such as `"Mana"`, of its holder: the Model or Player it's in, or that Player's character or that character's Player. Empty costs nothing.

#### CostAmount

```luau
Ability.CostAmount: number
```

How much of CostAttribute activating spends. It can't activate while its holder has less.

#### ActivationKey

```luau
Ability.ActivationKey: string
```

The input map action, such as `"Ability1"`, that activates it for the player whose character or Player holds it. The ability bar shows it. Empty: only scripts activate it.

#### BlockedByTags

```luau
Ability.BlockedByTags: string
```

Tags, separated by commas, such as `"Stunned, Silenced"`: while its holder has any of them, it can't activate.

#### GrantsTags

```luau
Ability.GrantsTags: string
```

Tags, separated by commas, its holder's character has while it's active (for its Duration).

#### Duration

```luau
Ability.Duration: number
```

Seconds it stays active after activating, giving its GrantsTags, before Ended fires. 0 ends at once.

#### Icon

```luau
Ability.Icon: string
```

The picture the ability bar shows for it, such as `"assets/textures/fireball.png"`. Empty shows its Name.

#### CooldownLeft

```luau
Ability.CooldownLeft: number
```

Read-only: seconds until it can activate again. 0 when it's ready.

#### Active

```luau
Ability.Active: boolean
```

Read-only: true from activating until Ended, for an Ability with a Duration.

### Events

#### Activated

```luau
Ability.Activated: Signal
```

Fires on the server and its players' devices as it activates, with the Player who asked (nil when a Script activated it). Do what it does here, such as the damage.

#### Ended

```luau
Ability.Ended: Signal
```

Fires as it stops being active, with true when Cancel ended it early. An Ability with no Duration ends as it activates.

#### CooldownChanged

```luau
Ability.CooldownChanged: Signal
```

Fires as its cooldown starts, with Cooldown, and as it ends, with 0.

### Functions

#### CanActivate

```luau
Ability:CanActivate(): (boolean, string?)
```

Whether it can activate now, and if not, why: it's cooling down, it's active, its holder has one of its BlockedByTags, or hasn't CostAmount of its CostAttribute.

#### Activate

```luau
Ability:Activate(): boolean
```

From a Script: activates it if it can (spends the cost, starts the cooldown, fires Activated) and returns whether it did. From a LocalScript: asks the server, which checks again, and returns whether it asked; the ability bar's sweep starts at once.

#### Cancel

```luau
Ability:Cancel(): ()
```

Scripts only: ends it early if it's active, taking off its GrantsTags and firing Ended with true. The cooldown carries on.

