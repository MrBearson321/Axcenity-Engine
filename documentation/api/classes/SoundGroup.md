---
description: "An audio bus: the Sounds whose SoundGroup is it, and the SoundGroups inside it, are mixed together at its Volume, through the sound effects inside it."
---

# SoundGroup

```luau
Instance.new("SoundGroup")
```

An audio bus: the Sounds whose SoundGroup is it, and the SoundGroups inside it, are mixed together at its Volume, through the sound effects inside it. Put them in SoundService, such as Master with Music and SFX inside.

It also has every member of [Instance](Instance.md).

### Properties

#### Volume

```luau
SoundGroup.Volume: number
```

A multiplier on everything played through it, as Sound.Volume: 1 leaves it as it is, 0 silences it. A SoundGroup inside another is a sub-bus, so both apply.

#### UserAdjustable

```luau
SoundGroup.UserAdjustable: boolean
```

If true, the default audio settings UI (the AudioSettings ScreenGui) has a volume slider for it.

