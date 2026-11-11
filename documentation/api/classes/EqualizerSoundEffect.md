---
description: "Inside a Sound or SoundGroup: raises or lowers its lows, mids and highs (LowGain, MidGain, HighGain, around MidRange)."
---

# EqualizerSoundEffect

```luau
Instance.new("EqualizerSoundEffect")
```

Inside a Sound or SoundGroup: raises or lowers its lows, mids and highs (LowGain, MidGain, HighGain, around MidRange).

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
EqualizerSoundEffect.Enabled: boolean
```

If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.

#### Priority

```luau
EqualizerSoundEffect.Priority: number
```

Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.

#### LowGain

```luau
EqualizerSoundEffect.LowGain: number
```

Decibels added below MidRange's X (negative turns them down), from -80 to 10.

#### MidGain

```luau
EqualizerSoundEffect.MidGain: number
```

Decibels added between MidRange's X and Y, from -80 to 10.

#### HighGain

```luau
EqualizerSoundEffect.HighGain: number
```

Decibels added above MidRange's Y, from -80 to 10.

#### MidRange

```luau
EqualizerSoundEffect.MidRange: Vector2
```

The mids, in hertz: from X to Y. 400 to 4000 by default.

