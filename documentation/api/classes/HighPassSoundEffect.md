---
description: "Inside a Sound or SoundGroup: lets through only what's above Cutoff, so it sounds thin, as through a radio."
---

# HighPassSoundEffect

```luau
Instance.new("HighPassSoundEffect")
```

Inside a Sound or SoundGroup: lets through only what's above Cutoff, so it sounds thin, as through a radio.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
HighPassSoundEffect.Enabled: boolean
```

If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.

#### Priority

```luau
HighPassSoundEffect.Priority: number
```

Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.

#### Cutoff

```luau
HighPassSoundEffect.Cutoff: number
```

Hertz below which the sound fades away: higher is thinner. 500 by default.

#### Resonance

```luau
HighPassSoundEffect.Resonance: number
```

From 0 to 1: how much it rings at Cutoff. 0 by default.

