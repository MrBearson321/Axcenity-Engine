---
description: "The service for the world's lighting."
---

# Lighting

```luau
game.Lighting
```

The service for the world's lighting. It sets sun shadows, the sky colors that metal parts reflect, and post-processing (exposure, tone mapping and bloom).

It also has every member of [Instance](Instance.md).

### Properties

#### ShadowsEnabled

```luau
Lighting.ShadowsEnabled: boolean
```

If false, the sun casts no shadows. Turn this off on low-end machines.

#### ShadowDistance

```luau
Lighting.ShadowDistance: number
```

How far from the camera sun shadows reach, in studs. Shorter distances give sharper shadows.

#### ShadowSoftness

```luau
Lighting.ShadowSoftness: number
```

How soft shadow edges are, from 0 (crisp) to 1 (very soft). Edges stay sharp where a part touches what it shades and soften further away, like real shadows.

#### AmbientOcclusionIntensity

```luau
Lighting.AmbientOcclusionIntensity: number
```

How strongly ambient occlusion darkens the ambient and sky light in corners and creases between parts, from 0 (not at all) to 2. 1 is normal. It's drawn only where the player's graphics settings turn ambient occlusion on, and never darkens sunlight, lamps or glowing parts.

#### SkyTopColor

```luau
Lighting.SkyTopColor: Color3
```

The sky's color straight up.

#### SkyHorizonColor

```luau
Lighting.SkyHorizonColor: Color3
```

The sky's color at the horizon.

#### GroundColor

```luau
Lighting.GroundColor: Color3
```

The color of the ground below the horizon, as the sky and reflections see it.

#### EnvironmentIntensity

```luau
Lighting.EnvironmentIntensity: number
```

How strongly the sky lights and is reflected by parts. 0 turns it off.

#### SkyImage

```luau
Lighting.SkyImage: string
```

A picture of the whole sky, such as "assets/sky/noon.hdr", shown behind the world and reflected by parts in place of the sky colors, with its light on everything. A Radiance .hdr file keeps the sun brighter than white; a PNG or JPEG also works. It wraps once around the world, its middle toward -Z and top straight up. Empty for the sky colors. Only drawn where the player's graphics settings have reflection quality above Low.

#### FogColor

```luau
Lighting.FogColor: Color3
```

The color far things fade into with distance fog (FogStart to FogEnd). Not used while Lighting has an Atmosphere, whose fog takes the sky's color.

#### FogStart

```luau
Lighting.FogStart: number
```

How many studs from the camera distance fog starts. Nearer is clear.

#### FogEnd

```luau
Lighting.FogEnd: number
```

How many studs from the camera everything is FogColor, including the sky. The default, 100000, is further than anything is seen, so there's no fog. An Atmosphere's fog takes its place.

#### PostProcessingEnabled

```luau
Lighting.PostProcessingEnabled: boolean
```

If false, the scene is drawn with no tone mapping or bloom.

#### Exposure

```luau
Lighting.Exposure: number
```

Brightens or darkens the whole image before tone mapping. 1 is neutral.

#### ToneMapping

```luau
Lighting.ToneMapping: EnumToneMapping | "Classic" | "Neutral" | "Filmic"
```

The curve that turns bright light into colors a screen can show. Classic (worlds made before it existed) keeps colors under 0.6 as they are and flattens brighter ones; Neutral (new worlds) keeps colors close to how they were picked and rolls highlights off without changing their hue; Filmic adds contrast for a cinematic look.

#### BloomEnabled

```luau
Lighting.BloomEnabled: boolean
```

If false, bright parts don't glow.

#### BloomIntensity

```luau
Lighting.BloomIntensity: number
```

How strong the glow around bright parts is.

#### BloomThreshold

```luau
Lighting.BloomThreshold: number
```

How bright a pixel must be before it glows.

#### ClockTime

```luau
Lighting.ClockTime: number
```

The time of day in hours, from 0 to 24 (25 becomes 1). The sun rises due east at 6, is highest at 12 and sets due west at 18; the moon, opposite it, lights the night, when the stars come out. Worlds made before it existed start at about 10.27, where their sun always was.

#### GeographicLatitude

```luau
Lighting.GeographicLatitude: number
```

How far north of the equator the world is, in degrees (negative is south). The noon sun stands this far from straight up, toward +Z north of the equator.

