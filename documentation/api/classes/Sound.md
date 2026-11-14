---
description: "A sound effect or music track that scripts can play."
---

# Sound

```luau
Instance.new("Sound")
```

A sound effect or music track that scripts can play. Spatial sounds sit on a part and get quieter with distance from the camera.

It also has every member of [Instance](Instance.md).

### Properties

#### SoundId

```luau
Sound.SoundId: string
```

The audio file inside the project folder, such as "assets/audio/chime.wav".

#### Volume

```luau
Sound.Volume: number
```

Playback volume as a non-negative amplitude; 1 is the original level and 0 is silent.

#### Looped

```luau
Sound.Looped: boolean
```

If true, playback repeats until Stop is called or the game session stops.

#### PlaybackSpeed

```luau
Sound.PlaybackSpeed: number
```

How fast the sound plays: 1 is normal, 2 twice as fast (and an octave higher), 0.5 half. Changes apply while it plays.

#### Playing

```luau
Sound.Playing: boolean
```

True while the sound plays. Set it to false to pause and true to carry on. Saved as true, the sound plays as soon as the game starts, with no script needed.

#### Spatial

```luau
Sound.Spatial: boolean
```

If true, the sound is placed in the world and panned as the camera moves. If false, it plays the same in both ears.

#### Position

```luau
Sound.Position: Vector3
```

Where a spatial sound plays from, in studs, when it is not inside a Part, MeshPart or Terrain. Inside one of those, that ancestor's Position is used instead.

#### RollOffMinDistance

```luau
Sound.RollOffMinDistance: number
```

A spatial sound stays at full volume within this many studs of the camera. 10 by default. Not used while Spatial is false.

#### RollOffMaxDistance

```luau
Sound.RollOffMaxDistance: number
```

A spatial sound is silent from this many studs out. 150 by default. Kept at least as large as RollOffMinDistance.

#### RollOffMode

```luau
Sound.RollOffMode: EnumRollOffMode | "Inverse" | "Linear" | "LinearSquare" | "InverseTapered"
```

How a spatial sound gets quieter between RollOffMinDistance and RollOffMaxDistance (`Enum.RollOffMode`): Inverse (the default, as sound behaves in the open: half as loud at twice the min distance), Linear, LinearSquare (quieter sooner) or InverseTapered (Inverse, fading out smoothly before the max distance).

#### SoundGroup

```luau
Sound.SoundGroup: Instance?
```

The SoundGroup it plays through, so that group's Volume and effects (and its parent groups') apply to it. nil plays it straight out.

### Events

#### Ended

```luau
Sound.Ended: Signal
```

Fires when the sound plays to its end. Looped sounds, and sounds stopped with Stop, don't fire it.

```lua
local music = workspace.Music
music.Ended:Connect(function()
	print("The song finished")
end)
music:Play()
```

### Functions

#### Play

```luau
Sound:Play(): ()
```

Plays this sound from the start, with its SoundId, Volume, PlaybackSpeed, Looped and Spatial, and sets Playing to true. Playing it again restarts it; stopping the game session stops all sounds.

```lua
local chime = workspace.Bell.Chime
chime:Play()
chime.Ended:Connect(function()
	print("Done")
end)
```

#### Stop

```luau
Sound:Stop(): ()
```

Stops this sound and sets Playing to false. The next Play starts from the beginning.

#### Pause

```luau
Sound:Pause(): ()
```

Pauses this sound where it is (sets Playing to false). Resume carries on from there.

#### Resume

```luau
Sound:Resume(): ()
```

Carries on playing a paused sound, or plays a stopped one from the start (sets Playing to true).

