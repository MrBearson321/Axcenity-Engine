---
description: "Inside a Sound or SoundGroup: turns down what's louder than Threshold by Ratio, so loud and quiet sit closer together."
---

# CompressorSoundEffect

```luau
Instance.new("CompressorSoundEffect")
```

Inside a Sound or SoundGroup: turns down what's louder than Threshold by Ratio, so loud and quiet sit closer together.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
CompressorSoundEffect.Enabled: boolean
```

If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.

#### Priority

```luau
CompressorSoundEffect.Priority: number
```

Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.

#### Threshold

```luau
CompressorSoundEffect.Threshold: number
```

Decibels, from -60 to 0, above which the sound is turned down. -40 by default.

#### Ratio

```luau
CompressorSoundEffect.Ratio: number
```

How much it's turned down above Threshold, from 1 (not at all) to 50: 4 lets through a quarter. 40 by default.

#### Attack

```luau
CompressorSoundEffect.Attack: number
```

Seconds it takes to turn the sound down once it's over Threshold. 0.1 by default.

#### Release

```luau
CompressorSoundEffect.Release: number
```

Seconds it takes to turn back up once it's under. 0.1 by default.

#### GainMakeup

```luau
CompressorSoundEffect.GainMakeup: number
```

Decibels, from 0 to 30, everything is turned back up by after.

