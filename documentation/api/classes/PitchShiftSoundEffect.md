---
description: "Inside a Sound or SoundGroup: raises or lowers its pitch by Octave without changing its speed."
---

# PitchShiftSoundEffect

```luau
Instance.new("PitchShiftSoundEffect")
```

Inside a Sound or SoundGroup: raises or lowers its pitch by Octave without changing its speed.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
PitchShiftSoundEffect.Enabled: boolean
```

If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.

#### Priority

```luau
PitchShiftSoundEffect.Priority: number
```

Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.

#### Octave

```luau
PitchShiftSoundEffect.Octave: number
```

How much higher it sounds, from 0.5 (an octave down) to 2 (an octave up), at the same speed. 1.25 by default.

