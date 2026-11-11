---
description: "Inside a Sound or SoundGroup: repeats it every Delay seconds, each repeat Feedback decibels quieter."
---

# EchoSoundEffect

```luau
Instance.new("EchoSoundEffect")
```

Inside a Sound or SoundGroup: repeats it every Delay seconds, each repeat Feedback decibels quieter.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
EchoSoundEffect.Enabled: boolean
```

If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.

#### Priority

```luau
EchoSoundEffect.Priority: number
```

Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.

#### Delay

```luau
EchoSoundEffect.Delay: number
```

Seconds between repeats, from 0.01 to 5. 1 by default.

#### Feedback

```luau
EchoSoundEffect.Feedback: number
```

Decibels each repeat is quieter than the one before, from -80 to 0. -3 by default.

#### DryLevel

```luau
EchoSoundEffect.DryLevel: number
```

Decibels of the sound as it is, from -80 to 20.

#### WetLevel

```luau
EchoSoundEffect.WetLevel: number
```

Decibels of the repeats, from -80 to 20.

