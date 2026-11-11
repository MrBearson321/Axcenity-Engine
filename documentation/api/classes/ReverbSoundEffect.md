---
description: "Inside a Sound or SoundGroup: makes it sound like a room, with DecayTime, Density, DryLevel and WetLevel."
---

# ReverbSoundEffect

```luau
Instance.new("ReverbSoundEffect")
```

Inside a Sound or SoundGroup: makes it sound like a room, with DecayTime, Density, DryLevel and WetLevel.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
ReverbSoundEffect.Enabled: boolean
```

If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.

#### Priority

```luau
ReverbSoundEffect.Priority: number
```

Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.

#### DecayTime

```luau
ReverbSoundEffect.DecayTime: number
```

Seconds the reverb's tail takes to die away, from 0.1 to 20. 1.5 by default.

#### Density

```luau
ReverbSoundEffect.Density: number
```

From 0 to 1: lower is darker and sparser, 1 (the default) a bright, smooth wash.

#### DryLevel

```luau
ReverbSoundEffect.DryLevel: number
```

Decibels of the sound as it is, from -80 to 20. -6 by default.

#### WetLevel

```luau
ReverbSoundEffect.WetLevel: number
```

Decibels of the reverb, from -80 to 20. 0 by default.

