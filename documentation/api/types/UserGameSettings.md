---
description: "This computer's graphics settings, from game:GetService(\"UserGameSettings\"), for a settings menu."
---

# UserGameSettings

This computer's graphics settings, from `game:GetService("UserGameSettings")`, for a settings menu. `GraphicsQuality` picks a preset, and each other field overrides one of its values; reading a field gives the value chosen (its override, or the preset's), and setting it to nil goes back to the preset's. Changing the preset keeps the overrides. Saved on this computer by the player (never in the game); in the editor's Play they start from the editor's Graphics settings and are put back at Stop. The GPU may draw less than was chosen: the player's log and the editor's Settings say why. LocalScripts only.

### Properties

#### GraphicsQuality

```luau
UserGameSettings.GraphicsQuality: EnumGraphicsQuality
```

The preset: Low, Medium, High or Ultra, or Automatic (High, or Low on a computer without a graphics card).

#### ShadowQuality

```luau
UserGameSettings.ShadowQuality: EnumShadowQuality
```

The sun's shadow map: Off, Low (1024² × 2 cascades), Medium (2048² × 2), High (2048² × 3) or Ultra (4096² × 4), and how many spot lights cast shadows (none up to Low, then 2, 4, 8) and point lights (none up to Low, then 1, 2, 4).

#### ShadowDistance

```luau
UserGameSettings.ShadowDistance: number
```

The farthest shadows reach, in studs: a ceiling on `Lighting.ShadowDistance`.

#### PostProcessing

```luau
UserGameSettings.PostProcessing: boolean
```

False turns bloom and tone mapping off, whatever Lighting asks for.

#### RenderScale

```luau
UserGameSettings.RenderScale: number
```

The scene's size as a fraction of the window's, from 0.5 to 2 (kept within that). The UI is always drawn at the window's size.

#### TextureQuality

```luau
UserGameSettings.TextureQuality: EnumTextureQuality
```

How sharp surface textures are: Full, Half or Quarter of their width and height.

#### AntiAliasing

```luau
UserGameSettings.AntiAliasing: EnumAntiAliasing
```

How edges are smoothed: Off, FXAA, MSAA2, MSAA4 or TAA (each frame drawn a fraction of a pixel aside and blended with the frames before, following what moves: smooths edges and thin shapes, a little soft). No preset picks TAA.

#### AmbientOcclusion

```luau
UserGameSettings.AmbientOcclusion: EnumAmbientOcclusion
```

How corners and creases darken their ambient light: Off, Low or High.

#### ReflectionQuality

```luau
UserGameSettings.ReflectionQuality: EnumReflectionQuality
```

How the sky is reflected and lights surfaces: Low works its gradient out per pixel (no sky image), Medium, High and Ultra bake it into a 64², 128² or 256² cubemap.

#### ContactShadows

```luau
UserGameSettings.ContactShadows: boolean
```

Short shadows found on screen for the small gaps the shadow maps miss, such as under a part resting on the floor. On at High and Ultra.

#### RealtimeGI

```luau
UserGameSettings.RealtimeGI: boolean
```

Light probe volumes traced again a few probes each frame, so their bounced light follows a moving sun or changing lamps. Off at every preset, and off on a GPU without compute shaders.

#### VolumetricFog

```luau
UserGameSettings.VolumetricFog: boolean
```

An Atmosphere's fog lit by the sun with its shadows (sun shafts) and by lamps. On at Ultra, and off on a GPU without compute shaders.

#### MasterVolume

```luau
UserGameSettings.MasterVolume: number
```

A multiplier on everything the game plays, from 0 (silent) to 1 (as mixed, the default). Unlike the graphics settings it's the game's: save slots keep it (as the SaveService setting `Audio/MasterVolume`), and the default audio settings UI has a slider for it.

#### CloudQuality

```luau
UserGameSettings.CloudQuality: EnumCloudQuality
```

How Lighting's Clouds are drawn: Low as a flat sheet, Medium, High and Ultra looked through in 16, 32 or 64 steps (High and Ultra with fine detail and light traced toward the sun). Every quality casts the same shadows.

#### DepthOfField

```luau
UserGameSettings.DepthOfField: EnumDepthOfField
```

How a DepthOfFieldEffect blurs: Off, Low (16 samples per pixel) or High (32). Off at Low, Low at Medium, High above.

#### Vignette

```luau
UserGameSettings.Vignette: boolean
```

Whether a VignetteEffect is drawn. Off at Low, on above.

#### FilmGrain

```luau
UserGameSettings.FilmGrain: boolean
```

Whether a FilmGrainEffect is drawn. Off at Low, on above.

#### ChromaticAberration

```luau
UserGameSettings.ChromaticAberration: boolean
```

Whether a ChromaticAberrationEffect is drawn. Off at Low, on above.

#### Sharpen

```luau
UserGameSettings.Sharpen: boolean
```

Whether a SharpenEffect is drawn. Off at Low, on above.

#### SunRays

```luau
UserGameSettings.SunRays: boolean
```

Whether a SunRaysEffect is drawn. Off at Low, on above.

#### MotionBlur

```luau
UserGameSettings.MotionBlur: EnumMotionBlur
```

How a MotionBlurEffect streaks what moves: Off, Low (8 samples per pixel) or High (16). Off at Low, Low at Medium, High above.

#### HardwareRayTracing

```luau
UserGameSettings.HardwareRayTracing: boolean
```

Whether the GPU is opened with hardware ray tracing, which every RayTraced setting needs. Off unless the player or the game turned it on (a project can make it the default on GPUs that can, in Settings > This project). A change is saved at once but takes effect at the game's next start: compare with RayTracingActive to tell the player to restart. Nil goes back to the default.

#### RayTracingSupported

```luau
UserGameSettings.RayTracingSupported: boolean
```

Read only. Whether this GPU can trace rays in hardware, whether or not it was opened with them: offer HardwareRayTracing only where this is true.

#### RayTracingActive

```luau
UserGameSettings.RayTracingActive: boolean
```

Read only. Whether the GPU was opened with hardware ray tracing this run, so the RayTraced settings draw. False after HardwareRayTracing is turned on until the game starts again.

#### RayTracedShadows

```luau
UserGameSettings.RayTracedShadows: EnumRayTracedShadows
```

Hardware ray traced sun shadows: Off, Low (1 ray per pixel), Medium (2) or High (4). Off at every preset. With Hardware ray tracing off (the player's setting) or on a GPU that can't ray trace, it reads as chosen but the shadow maps are drawn. Lighting's ShadowSoftness still sets how soft they are.

#### RayTracedAmbientOcclusion

```luau
UserGameSettings.RayTracedAmbientOcclusion: EnumRayTracedAmbientOcclusion
```

Hardware ray traced ambient occlusion: Off, Low (2 rays per pixel), Medium (4) or High (8). Off at every preset. On, ambient occlusion is drawn even with AmbientOcclusion off, and things off screen darken corners too. With Hardware ray tracing off (the player's setting) or on a GPU that can't ray trace, it reads as chosen but only AmbientOcclusion's screen-space occlusion is drawn.

#### RayTracedGlobalIllumination

```luau
UserGameSettings.RayTracedGlobalIllumination: EnumRayTracedGlobalIllumination
```

Ray traced global illumination: Off, Low (128 light probes a frame, 64 rays each), Medium (256, 128 rays) or High (512, 256 rays). Off at every preset. On, light probes are traced with rays each frame, so bounced light follows the sun, lamps and moving parts: a grid that follows the camera (so no LightProbeVolume is needed) and the world's LightProbeVolumes. With Hardware ray tracing off (the player's setting) or on a GPU that can't ray trace, the rays are traced in software instead: slower (Low suits it), with leaves' holes guessed. Only a GPU without compute shaders keeps the bakes (or real-time GI, as set).

