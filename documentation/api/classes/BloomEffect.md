---
description: "Inside a GraphicsVolume: how bright light blooms while the camera is in the volume, in place of Lighting's BloomIntensity and BloomThreshold."
---

# BloomEffect

```luau
Instance.new("BloomEffect")
```

Inside a GraphicsVolume: how bright light blooms while the camera is in the volume, in place of Lighting's BloomIntensity and BloomThreshold.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
BloomEffect.Enabled: boolean
```

If false, it changes nothing.

#### Intensity

```luau
BloomEffect.Intensity: number
```

How much of the bloom is added to the picture, in place of Lighting's BloomIntensity (0.35 by default). 0 turns bloom off there.

#### Threshold

```luau
BloomEffect.Threshold: number
```

Only light brighter than this blooms, in place of Lighting's BloomThreshold (1 by default).

