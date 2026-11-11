---
description: "Inside a Part, MeshPart, Model, ScreenGui or UI element: plays animation on its parent while the game runs: its state machine, if it has one, and the AnimationTracks loaded with LoadAnimation."
---

# Animator

```luau
Instance.new("Animator")
```

Inside a Part, MeshPart, Model, ScreenGui or UI element: plays animation on its parent while the game runs: its state machine, if it has one, and the AnimationTracks loaded with LoadAnimation.

It also has every member of [Instance](Instance.md).

### Properties

#### StateMachine

```luau
Animator.StateMachine: string
```

The state machine file it plays on its parent, inside the project folder, such as "assets/animations/hero/hero.statemachine.json". Empty plays only the AnimationTracks loaded on it. The Animator tab makes and edits them.

#### Enabled

```luau
Animator.Enabled: boolean
```

If false, everything it animates holds still: the animator stops moving on.

#### Speed

```luau
Animator.Speed: number
```

Plays everything this many times as fast (1 is normal, 0 pauses).

#### CurrentState

```luau
Animator.CurrentState: string
```

The name of the state playing (read-only; use `Play` to change it). Empty until the animator first runs.

### Events

#### StateChanged

```luau
Animator.StateChanged: Signal
```

Fires when the animator moves to another state. The handler gets the new state's name, then the old one's: `function(state, previous) ... end`.

#### AnimationEvent

```luau
Animator.AnimationEvent: Signal
```

Fires when a playing clip passes one of its animation events. The handler gets the event's name.

### Functions

#### SetBool

```luau
Animator:SetBool(name: string, value: boolean): ()
```

Sets a Bool parameter, such as `IsGrounded`: something that stays true or false until it's changed.

#### GetBool

```luau
Animator:GetBool(name: string): boolean
```

A Bool parameter's value.

#### SetNumber

```luau
Animator:SetNumber(name: string, value: number): ()
```

Sets a Number parameter, such as `Speed`: a value that changes gradually, for conditions and blend trees.

#### GetNumber

```luau
Animator:GetNumber(name: string): number
```

A Number parameter's value.

#### SetTrigger

```luau
Animator:SetTrigger(name: string): ()
```

Sets a Trigger parameter, such as `Jump`: a one-time request, used up by the transition it starts.

```lua
local animator = workspace.Hero.Animator
animator:SetNumber("Speed", 12)
animator:SetTrigger("Jump")
```

#### ResetTrigger

```luau
Animator:ResetTrigger(name: string): ()
```

Clears a Trigger that no transition has used yet.

#### SetLayerWeight

```luau
Animator:SetLayerWeight(name: string, weight: number, fadeTime: number?): ()
```

Sets how much the state machine layer named `name` counts, from 0 (not at all) to 1 (fully), fading there over `fadeTime` seconds (at once if left out), such as an `UpperBody` layer's reload over a run.

#### GetLayerWeight

```luau
Animator:GetLayerWeight(name: string): number
```

How much the state machine layer named `name` counts now, from 0 to 1 (0 for a layer the Animator hasn't started).

#### Play

```luau
Animator:Play(state: string, blend: number?): ()
```

Switches to the state named `state`, from its start, blending from the pose now over `blend` seconds (0.25 if left out; 0 snaps).

#### LoadAnimation

```luau
Animator:LoadAnimation(animation: Animation): AnimationTrack
```

Loads `animation` (an Animation, whose Clip names the clip) on this Animator and returns an AnimationTrack to play, stop and blend over the state machine.

```lua
local animator = workspace.Hero.Animator
local wave = animator:LoadAnimation(workspace.Hero.Wave)
wave:Play(0.2)
```

#### GetPlayingAnimationTracks

```luau
Animator:GetPlayingAnimationTracks(): {AnimationTrack}
```

The AnimationTracks loaded on this Animator that are playing now.

