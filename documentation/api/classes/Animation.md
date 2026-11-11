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

Fires when `animation:Play()` reaches the end of a clip that doesn't loop. The handler gets no arguments.

#### AnimationEvent

```luau
Animation.AnimationEvent: Signal
```

Fires when `animation:Play()` passes one of the clip's events. The handler gets the event's name.

### Functions

#### Play

```luau
Animation:Play(): ()
```

Plays this Animation's clip from the start, through the Animator on the same parent. `animator:LoadAnimation(animation)` is the newer way, with fades and weights.

#### Stop

```luau
Animation:Stop(): ()
```

Stops playback and goes back to the start of the clip.

#### Pause

```luau
Animation:Pause(): ()
```

Pauses at the current time. Resume continues from there.

#### Resume

```luau
Animation:Resume(): ()
```

Continues after Pause.

