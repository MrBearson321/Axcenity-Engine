---
description: "Inside a Sound or SoundGroup: overdrives it, by Level."
---

# DistortionSoundEffect

```luau
Instance.new("DistortionSoundEffect")
```

Inside a Sound or SoundGroup: overdrives it, by Level.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
DistortionSoundEffect.Enabled: boolean
```

If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.

#### Priority

```luau
DistortionSoundEffect.Priority: number
```

Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.

#### Level

```luau
DistortionSoundEffect.Level: number
```

How much it's overdriven, from 0 (clean) to 1 (fuzz). 0.75 by default.

