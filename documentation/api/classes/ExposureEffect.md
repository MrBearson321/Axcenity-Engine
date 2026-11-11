---
description: "Inside a GraphicsVolume: the exposure while the camera is in the volume, in place of Lighting's Exposure."
---

# ExposureEffect

```luau
Instance.new("ExposureEffect")
```

Inside a GraphicsVolume: the exposure while the camera is in the volume, in place of Lighting's Exposure.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
ExposureEffect.Enabled: boolean
```

If false, it changes nothing.

#### Exposure

```luau
ExposureEffect.Exposure: number
```

Multiplies the scene's brightness before tone mapping, in place of Lighting's Exposure (1 by default).

