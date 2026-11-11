---
description: "Inside Lighting: the sky is drawn as sunlight scattered by air in place of the sky colors: blue by day, red and orange toward sunset, dark and starry at night as ClockTime goes round."
---

# Atmosphere

```luau
Instance.new("Atmosphere")
```

Inside Lighting: the sky is drawn as sunlight scattered by air in place of the sky colors: blue by day, red and orange toward sunset, dark and starry at night as ClockTime goes round. The air also colors the sun's light and lights and is reflected by parts. Far parts fade into its fog, the sky's color around the horizon, thickest low down; at Ultra graphics the fog shows the sun's shadows (sun shafts) and lamps' light. Lighting's own FogStart and FogEnd aren't used while it's there. A SkyImage is still drawn in its place, but the sun's light is colored by the air.

It also has every member of [Instance](Instance.md).

### Properties

#### Density

```luau
Atmosphere.Density: number
```

How thick the air is, from 0 (none: a black sky and a white sun, and no fog) to 1. 0.395 is Earth's: its fog hides half of what's 500 studs away at ground level.

#### Offset

```luau
Atmosphere.Offset: number
```

How high above the ground the sky is seen from, from 0 to 1 (one height of the air up, where the sky above darkens). The fog is thickest at the bottom of the air and thins with height, every 50 studs by about a third; Offset lifts the world above that bottom (by 250 studs at 1), so the fog thins.

#### Color

```luau
Atmosphere.Color: Color3
```

Tints the whole sky. The default grey leaves it as the air scatters it.

#### Decay

```luau
Atmosphere.Decay: Color3
```

Tints the sky away from the sun. The default blue-grey leaves it as the air scatters it.

#### Glare

```luau
Atmosphere.Glare: number
```

Brightens the glow around the sun, from 0 to 10, and tightens its glow in the fog.

#### Haze

```luau
Atmosphere.Haze: number
```

Adds dust and haze to the air, from 0 to 10: a paler horizon, a wider glow around the sun, a dimmer, redder sun, and thicker fog (1 + Haze times as thick).

