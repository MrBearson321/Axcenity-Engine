---
description: "Inside a GraphicsVolume: distance fog from Start to End studs in Color while the camera is in the volume, in place of the world's fog."
---

# FogEffect

```luau
Instance.new("FogEffect")
```

Inside a GraphicsVolume: distance fog from Start to End studs in Color while the camera is in the volume, in place of the world's fog.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
FogEffect.Enabled: boolean
```

If false, it changes nothing.

#### Color

```luau
FogEffect.Color: Color3
```

The fog's color. Light grey by default.

#### Start

```luau
FogEffect.Start: number
```

Studs from the camera where the fog begins (0 by default).

#### End

```luau
FogEffect.End: number
```

Studs from the camera where everything is the fog's color (200 by default).

