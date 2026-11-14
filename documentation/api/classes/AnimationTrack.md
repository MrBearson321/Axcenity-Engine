---
description: "One Animation loaded on an Animator, from animator:LoadAnimation(animation): play, stop and blend it over the state machine."
---

# AnimationTrack

```luau
Instance.new("AnimationTrack")
```

One Animation loaded on an Animator, from `animator:LoadAnimation(animation)`: play, stop and blend it over the state machine.

It also has every member of [Instance](Instance.md).

### Properties

#### IsPlaying

```luau
AnimationTrack.IsPlaying: boolean
```

True from Play until Stop, or until a clip that doesn't loop ends (read-only).

#### Length

```luau
AnimationTrack.Length: number
```

The clip's length in seconds (read-only; 0 until the clip has been read, a frame after LoadAnimation).

#### Looped

```luau
AnimationTrack.Looped: boolean
```

Whether it plays again from the start when it ends. Starts as the clip's own.

#### Speed

```luau
AnimationTrack.Speed: number
```

How fast it plays (read-only; use AdjustSpeed).

#### TimePosition

```luau
AnimationTrack.TimePosition: number
```

Seconds into the clip. Set it to jump there.

#### WeightCurrent

```luau
AnimationTrack.WeightCurrent: number
```

How much it counts now, from 0 to 1, while fading (read-only).

#### WeightTarget

```luau
AnimationTrack.WeightTarget: number
```

The weight it's fading to (read-only; use AdjustWeight).

#### Animation

```luau
AnimationTrack.Animation: Animation
```

The Animation it was loaded from (read-only).

### Events

#### Stopped

```luau
AnimationTrack.Stopped: Signal
```

Fires when the track has stopped: after Stop's fade, or at the end of a clip that doesn't loop.

```lua
local animator = workspace.Hero.Humanoid.Animator
local wave = animator:LoadAnimation(workspace.Hero.Wave)
wave.Stopped:Connect(function()
	print("Done waving")
end)
wave:Play()
```

#### KeyframeReached

```luau
AnimationTrack.KeyframeReached: Signal
```

Fires each time the track passes one of the clip's events. The handler gets the event's name; GetMarkerReachedSignal hears one name.

### Functions

#### Play

```luau
AnimationTrack:Play(fadeTime: number?, weight: number?, speed: number?): ()
```

Plays the clip from its start, fading in over `fadeTime` seconds (0.1 if left out) to `weight` (1), at `speed` (1). Where it and what plays under it animate the same thing, the track started last wins.

#### Stop

```luau
AnimationTrack:Stop(fadeTime: number?): ()
```

Stops it, fading out over `fadeTime` seconds (0.1 if left out). Stopped fires once it has faded out.

#### AdjustSpeed

```luau
AnimationTrack:AdjustSpeed(speed: number?): ()
```

Plays it this many times as fast from now (1 is normal, 0 holds, negative plays backwards).

#### AdjustWeight

```luau
AnimationTrack:AdjustWeight(weight: number?, fadeTime: number?): ()
```

Fades how much it counts to `weight` (0 to 1) over `fadeTime` seconds (0.1 if left out).

#### GetMarkerReachedSignal

```luau
AnimationTrack:GetMarkerReachedSignal(name: string): Signal
```

An event that fires each time the track passes the clip's event named `name`. The handler gets the name.

