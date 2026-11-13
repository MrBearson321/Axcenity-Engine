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

#### SizeOverLife

```luau
ParticleEmitter.SizeOverLife: NumberSequence?
```

A NumberSequence of studs across over each particle's life, from birth (0) to death (1), in place of Size and SizeEnd. nil (the default) uses them.

#### ColorOverLife

```luau
ParticleEmitter.ColorOverLife: ColorSequence?
```

A ColorSequence over each particle's life, in place of Color and ColorEnd. nil (the default) uses them.

#### TransparencyOverLife

```luau
ParticleEmitter.TransparencyOverLife: NumberSequence?
```

A NumberSequence from 0 (solid) to 1 (invisible) over each particle's life, in place of Transparency and TransparencyEnd. nil (the default) uses them.

#### SpeedOverLife

```luau
ParticleEmitter.SpeedOverLife: NumberSequence?
```

A NumberSequence that multiplies Speed over each particle's life, from birth (0) to death (1): how far it moves each moment, with Acceleration and Drag still acting on its velocity. Envelopes aren't used. nil (the default) keeps Speed.

#### Shape

```luau
ParticleEmitter.Shape: EnumParticleEmitterShape | "Box" | "Sphere" | "Cylinder" | "Disc"
```

Where in the part particles are born: `Enum.ParticleEmitterShape.Box` (the default, the whole part), `Sphere` (the ball inside it), `Cylinder` (upright along the part's Y) or `Disc` (flat across its middle, facing its Y). A Box's particles head along Direction within SpreadAngle; the round shapes' head away from their middle (see ShapeInOut).

#### ShapeStyle

```luau
ParticleEmitter.ShapeStyle: EnumParticleEmitterShapeStyle | "Volume" | "Surface"
```

`Enum.ParticleEmitterShapeStyle.Volume` (the default) gives off particles anywhere inside the Shape; `Surface` only on it: a box's faces, the sphere, a cylinder's round side or a disc's rim.

#### ShapeInOut

```luau
ParticleEmitter.ShapeInOut: EnumParticleEmitterShapeInOut | "Outward" | "Inward" | "InAndOut"
```

Which way particles head from the Shape, within SpreadAngle: `Enum.ParticleEmitterShapeInOut.Outward` (the default: along Direction for a Box; away from the middle for a Sphere, and from the upright axis for a Cylinder or Disc), `Inward` (the other way) or `InAndOut` (either, at random).

#### VelocityInheritance

```luau
ParticleEmitter.VelocityInheritance: number
```

How much of the part's own velocity each particle starts with, from 0 (none, the default) to 1 (all of it), so sparks from a moving car keep up with it.

#### Collides

```luau
ParticleEmitter.Collides: boolean
```

On, particles bounce off parts with CanCollide on and the ground (not off the emitter's own part, nor parts with CanCollide off), and come to rest on a floor. Colliding emitters move their particles on the CPU, at most 4,000 each. Default false.

#### Bounce

```luau
ParticleEmitter.Bounce: number
```

How much of a colliding particle's speed into a surface it keeps, back out of it: 0 lands dead, 1 bounces as high as it fell. Speed along the surface is kept. Default 0.5.

#### LifetimeLoss

```luau
ParticleEmitter.LifetimeLoss: number
```

How much of its life a colliding particle loses each time it lands, from 0 (none, the default) to 1 (it dies where it lands).

#### EmitOn

```luau
ParticleEmitter.EmitOn: EnumParticleEmitterEmitOn | "Always" | "Birth" | "Death" | "Collision"
```

Inside another ParticleEmitter, `Enum.ParticleEmitterEmitOn.Birth`, `Death` or `Collision` makes this one a sub-emitter: while Enabled it gives off EmitCount particles where each of its parent's particles is born, dies or lands, heading out along its own Direction (turned by its Orientation), and nothing by its Rate. `Always` (the default) gives off as any emitter does.

#### EmitCount

```luau
ParticleEmitter.EmitCount: number
```

How many particles a sub-emitter (EmitOn Birth, Death or Collision) gives off at each of its parent's events. Default 10.

#### Bursts

```luau
ParticleEmitter.Bursts: NumberSequence?
```

A NumberSequence of bursts while Enabled: each keypoint gives off Value particles (give or take Envelope) at its Time of every Duration seconds, over and over. Rate still adds its steady flow. A keypoint at time 1 is the next loop's start, so it gives off nothing: put that count at time 0. nil (the default) is no bursts.

#### Duration

```luau
ParticleEmitter.Duration: number
```

How long one loop of Bursts takes, in seconds (default 1, at least 0.05).

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

#### FlipbookLayout

```luau
ParticleEmitter.FlipbookLayout: EnumParticleFlipbookLayout | "None" | "Grid2x2" | "Grid4x4" | "Grid8x8"
```

Cuts the Texture into a grid of frames that each particle shows one at a time, read left to right, then top to bottom: `Enum.ParticleFlipbookLayout.Grid2x2`, `Grid4x4` or `Grid8x8` (4, 16 or 64 frames). `None` (the default) shows the whole Texture. The built-in ParticleType pictures have no frames, so it needs a Texture.

#### FlipbookMode

```luau
ParticleEmitter.FlipbookMode: EnumParticleFlipbookMode | "Loop" | "OneShot" | "PingPong" | "Random"
```

Which frame a particle shows as it ages: `Enum.ParticleFlipbookMode.Loop` (the default: in order at FlipbookFramerate, then from the first again), `OneShot` (every frame once over its life, whatever the frame rate), `PingPong` (in order, then back) or `Random` (a frame at random, again at the frame rate).

#### FlipbookFramerate

```luau
ParticleEmitter.FlipbookFramerate: number
```

How many flipbook frames a second each particle moves on (default 1). OneShot ignores it.

#### FlipbookStartRandom

```luau
ParticleEmitter.FlipbookStartRandom: boolean
```

When true, each particle starts on a frame of its own instead of the first (Loop and PingPong), so a puff of smoke doesn't animate in step.

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

