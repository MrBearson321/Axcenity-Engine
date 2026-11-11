---
description: "Gives off particles (sparks, smoke, fire, stars...) from inside its parent part, shown in the editor and the game."
---

# ParticleEmitter

```luau
Instance.new("ParticleEmitter")
```

Gives off particles (sparks, smoke, fire, stars...) from inside its parent part, shown in the editor and the game.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
ParticleEmitter.Enabled: boolean
```

If false, it stops giving off particles; those already out live on.

#### ParticleType

```luau
ParticleEmitter.ParticleType: EnumParticleType | "Circle" | "Spark" | "Smoke" | "Fire" | "Star" | "Magic"
```

The built-in picture each particle shows: Circle, Spark, Smoke, Fire, Star or Magic. Texture, if set, is used instead.

#### Texture

```luau
ParticleEmitter.Texture: string
```

An image file inside the project folder each particle shows instead of ParticleType, such as "assets/particles/spark_01.png" (Kenney's free Particle Pack works well). Its alpha is the shape; it's tinted by Color.

#### Rate

```luau
ParticleEmitter.Rate: number
```

How many particles a second it gives off while Enabled.

#### Lifetime

```luau
ParticleEmitter.Lifetime: number
```

How many seconds each particle lives (each one a little more or less).

#### Speed

```luau
ParticleEmitter.Speed: number
```

How fast particles leave, in studs a second, along Direction.

#### Direction

```luau
ParticleEmitter.Direction: Vector3
```

The way particles leave, relative to the parent part (up by default).

#### SpreadAngle

```luau
ParticleEmitter.SpreadAngle: number
```

How wide the cone particles leave in is, in degrees: 0 is a straight line, 360 every way.

#### Acceleration

```luau
ParticleEmitter.Acceleration: Vector3
```

Added to every particle's speed each second, in world studs (gravity is Vector3.new(0, -196.2, 0)).

#### Drag

```luau
ParticleEmitter.Drag: number
```

How quickly particles slow down: 0 never, higher sooner.

#### WindAffectsDrag

```luau
ParticleEmitter.WindAffectsDrag: boolean
```

If true, Drag slows particles toward workspace.GlobalWind instead of to a stop, so the wind carries them. Needs a Drag above 0.

#### Size

```luau
ParticleEmitter.Size: number
```

How many studs across a particle is when it's born.

#### SizeEnd

```luau
ParticleEmitter.SizeEnd: number
```

How many studs across a particle is when it dies; it grows or shrinks smoothly.

#### Color

```luau
ParticleEmitter.Color: Color3
```

A particle's color when it's born. It tints the picture.

#### ColorEnd

```luau
ParticleEmitter.ColorEnd: Color3
```

A particle's color when it dies; it fades smoothly from Color.

#### Transparency

```luau
ParticleEmitter.Transparency: number
```

From 0 (solid) to 1 (invisible) when a particle is born.

#### TransparencyEnd

```luau
ParticleEmitter.TransparencyEnd: number
```

From 0 (solid) to 1 (invisible) when a particle dies.

#### Rotation

```luau
ParticleEmitter.Rotation: number
```

How far each particle is turned when it's born, in degrees.

#### RotSpeed

```luau
ParticleEmitter.RotSpeed: number
```

How fast particles spin, in degrees a second.

#### LightEmission

```luau
ParticleEmitter.LightEmission: number
```

From 0 (particles paint over what's behind them, like smoke) to 1 (they add their light, like fire and sparks).

#### Brightness

```luau
ParticleEmitter.Brightness: number
```

How brightly particles glow: 1 is their color as it is; higher values bloom.

#### Position

```luau
ParticleEmitter.Position: Vector3
```

Where it is in the world when it isn't inside a part, in studs. Inside a part it works from the part and this isn't used.

#### Orientation

```luau
ParticleEmitter.Orientation: Vector3
```

How it's turned when it isn't inside a part, in degrees, as a part's Orientation. Inside a part it turns with the part.

#### Locked

```luau
ParticleEmitter.Locked: boolean
```

If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.

### Functions

#### Emit

```luau
ParticleEmitter:Emit(count: number): ()
```

Gives off this many particles at once (up to 1000), even while Enabled is false, such as a burst of sparks when something breaks.

```lua
workspace.Fireworks.ParticleEmitter:Emit(50)
```

