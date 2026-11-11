---
description: "Inside Lighting: rain or snow falling around the camera, and how wet and snow-covered surfaces are (parts, models and terrain)."
---

# Weather

```luau
Instance.new("Weather")
```

Inside Lighting: rain or snow falling around the camera, and how wet and snow-covered surfaces are (parts, models and terrain). Wet surfaces are darker and glossier; snow covers the tops of things but not walls. workspace.GlobalWind blows the rain and snow. Every change eases in over TransitionTime. With Precipitation None and no Wetness or SnowCover it costs nothing.

It also has every member of [Instance](Instance.md).

### Properties

#### Precipitation

```luau
Weather.Precipitation: EnumPrecipitation | "None" | "Rain" | "Snow"
```

What falls around the camera (`Enum.Precipitation`): None, Rain (the default) or Snow. Rain wets surfaces as it falls, up to its Intensity; snow covers the ones facing up.

#### Intensity

```luau
Weather.Intensity: number
```

How hard the rain or snow falls, from 0 to 1 (0.5 by default): more drops, and wetter or deeper on the ground.

#### Wetness

```luau
Weather.Wetness: number
```

How wet surfaces are whatever is falling, from 0 (dry, the default) to 1 (soaked): darker and glossier. Rain wets them at least as much as its Intensity.

#### SnowCover

```luau
Weather.SnowCover: number
```

How much snow lies on surfaces facing up whatever is falling, from 0 (none, the default) to 1: more cover whitens flat tops fully and reaches further down slopes. Walls stay bare. Falling snow covers them at least as much as its Intensity.

#### TransitionTime

```luau
Weather.TransitionTime: number
```

Seconds a change takes to ease in (10 by default): rain starting and stopping, surfaces wetting and drying, snow settling and melting. 0 changes at once.

