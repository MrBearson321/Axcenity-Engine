---
description: "How much a Model can take: Health out of MaxHealth, put in any Model."
---

# Health

```luau
Instance.new("Health")
```

How much a Model can take: Health out of MaxHealth, put in any Model. TakeDamage, DamageZones and falls (with FallDamage on) lower it; Heal and regeneration (RegenRate, after RegenDelay seconds without damage) raise it. At 0 it fires Died once and the Model is left for scripts to handle.

It also has every member of [Instance](Instance.md).

### Properties

#### MaxHealth

```luau
Health.MaxHealth: number
```

The most Health it can have. Heal and regeneration stop here, and lowering it lowers a Health above it.

#### Health

```luau
Health.Health: number
```

How much is left, 0 to MaxHealth. Setting it fires HealthChanged (and Died, if it reaches 0). At 0 the Model is dead: damage and Heal do nothing, and it doesn't regenerate, until a script sets Health again.

#### RegenRate

```luau
Health.RegenRate: number
```

Health a second won back once RegenDelay seconds pass without damage, up to MaxHealth. 0 never regenerates.

#### RegenDelay

```luau
Health.RegenDelay: number
```

Seconds without damage before it starts regenerating. Any damage, from TakeDamage, a DamageZone or a fall, starts the wait again.

#### Invulnerable

```luau
Health.Invulnerable: boolean
```

If true, TakeDamage, DamageZones and falls do nothing to it. Heal still heals.

#### FallDamage

```luau
Health.FallDamage: boolean
```

If true, the character controller in its Model loses health when it lands faster than FallDamageThreshold: FallDamageScale for each stud a second over it. Off by default.

#### FallDamageThreshold

```luau
Health.FallDamageThreshold: number
```

Studs a second a character can land at without damage. At the default Gravity, 60 is a fall of about 9 studs; a jump lands at about 53.

#### FallDamageScale

```luau
Health.FallDamageScale: number
```

Health lost for each stud a second a landing is over FallDamageThreshold.

#### DisplayName

```luau
Health.DisplayName: string
```

The name a game shows over the character. Empty is the Model's Name. A Humanoid's DisplayName is this.

#### HealthDisplayDistance

```luau
Health.HealthDisplayDistance: number
```

How far away, in studs, a game shows the character's health over it; 0 never. A Humanoid's HealthDisplayDistance is this.

### Events

#### HealthChanged

```luau
Health.HealthChanged: Signal
```

Fires whenever Health changes: damage, Heal, regeneration or a script setting it. The handler gets the new health and the old.

#### Damaged

```luau
Health.Damaged: Signal
```

Fires when it takes damage, after HealthChanged. The handler gets how much health it lost and what did it: the `source` given to TakeDamage, the DamageZone, or nil (a fall, or no source).

#### Died

```luau
Health.Died: Signal
```

Fires once when Health reaches 0, after HealthChanged, however many hits landed that frame. Nothing else happens to the Model unless it has a Humanoid: scripts decide what dying does. It can fire again only after a script sets Health above 0.

### Functions

#### TakeDamage

```luau
Health:TakeDamage(amount: number, source: Instance?): ()
```

Takes `amount` from Health (never below 0), unless it's Invulnerable or already dead. Fires HealthChanged, then Damaged with `amount` and `source`, then Died if it reached 0. Starts the RegenDelay wait again.

#### Heal

```luau
Health:Heal(amount: number): ()
```

Adds `amount` to Health, up to MaxHealth, and fires HealthChanged. Does nothing once it's dead (Health 0): set Health to bring it back.

