---
description: "Inside a GraphicsVolume: distance fog from Start to End studs in Color while the camera is in the volume, in place of the world's fog."
---

# FogEffect

```luau
Instance.new("FogEffect")
```

Inside a GraphicsVolume: distance fog from Start to End studs in Color while the camera is in the volume, in place of the world's fog. With a Density above 0, dust or mist filling the volume's box instead, seen from anywhere and lit by the sun (shafts of light through windows and gaps); with Interior on, the volume clears the outdoor fog inside it.

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

#### Density

```luau
FogEffect.Density: number
```

Above 0, the fog is dust or mist filling the volume's box, seen from anywhere, instead of distance fog: it takes away this much of the light per stud (0.02 hides about half of what's 35 studs away), and Color is how much of the light it takes it scatters back. It's lit by the sun and its shadows (shafts of light, with volumetric fog on), by lamps and by the light around it (light probes), and it thins to nothing over BlendDistance at the box's faces. 0 by default.

#### Glare

```luau
FogEffect.Glare: number
```

How much of the dust's light goes forward, away from the sun, from 0 to 10 (0 by default): more makes it glow brighter looking toward the sun.

#### HeightFalloff

```luau
FogEffect.HeightFalloff: number
```

Studs above the volume's bottom over which its dust thins by e, for haze lying low in a valley; 0 (the default) keeps it even all the way up.

#### Interior

```luau
FogEffect.Interior: boolean
```

Makes the volume an interior (a room, a building): inside its box the air outside is cleared (an Atmosphere's fog, Lighting's fog and lower volumes' dust), and only this effect's own Density fills it, 0 for clear air. False by default.

