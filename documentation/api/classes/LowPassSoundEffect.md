---
description: "Inside a Sound or SoundGroup: lets through only what's below Cutoff, so it sounds muffled, as through a wall or under a pause menu."
---

# LowPassSoundEffect

```luau
Instance.new("LowPassSoundEffect")
```

Inside a Sound or SoundGroup: lets through only what's below Cutoff, so it sounds muffled, as through a wall or under a pause menu.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
LowPassSoundEffect.Enabled: boolean
```

If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.

#### Priority

```luau
LowPassSoundEffect.Priority: number
```

Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.

#### Cutoff

```luau
LowPassSoundEffect.Cutoff: number
```

Hertz above which the sound fades away: lower is more muffled. 1000 by default.

#### Resonance

```luau
LowPassSoundEffect.Resonance: number
```

From 0 to 1: how much it rings at Cutoff. 0 by default.

