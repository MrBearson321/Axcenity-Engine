---
description: "Points at a clip (*.anim.json)."
---

# Animation

```luau
Instance.new("Animation")
```

Points at a clip (`*.anim.json`). Load it with `animator:LoadAnimation(animation)` to play it, or tick AutoPlay to play it when the game starts.

It also has every member of [Instance](Instance.md).

### Properties

#### Clip

```luau
Animation.Clip: string
```

The clip file inside the project folder, such as "assets/animations/Door/Open.anim.json".

#### AutoPlay

```luau
Animation.AutoPlay: boolean
```

If true, the Animator on the same parent plays it by itself when the game starts.

#### Enabled

```luau
Animation.Enabled: boolean
```

If false, playback pauses at its current time; set true to continue.

#### Speed

```luau
Animation.Speed: number
```

Plays the clip this many times as fast (1 is normal, 0 pauses).

#### Looped

```luau
Animation.Looped: boolean
```

If true, playback repeats instead of firing Completed at the end.

### Events

#### Completed

```luau
Animation.Completed: Signal
```

Fires when the Animation, played by AutoPlay, reaches the end of a clip that doesn't loop. The handler gets no arguments.

#### AnimationEvent

```luau
Animation.AnimationEvent: Signal
```

Fires when the Animation, played by AutoPlay, passes one of the clip's events. The handler gets the event's name.

### Functions

#### Play

```luau
Animation:Play(): ()
```

Removed (an error that names the fix): `animator:LoadAnimation(animation)` gives an AnimationTrack to play, stop and pause instead.

#### Stop

```luau
Animation:Stop(): ()
```

Removed (an error that names the fix): `animator:LoadAnimation(animation)` gives an AnimationTrack to play, stop and pause instead.

#### Pause

```luau
Animation:Pause(): ()
```

Removed (an error that names the fix): `animator:LoadAnimation(animation)` gives an AnimationTrack to play, stop and pause instead.

#### Resume

```luau
Animation:Resume(): ()
```

Removed (an error that names the fix): `animator:LoadAnimation(animation)` gives an AnimationTrack to play, stop and pause instead.

