---
description: Clips, state machines, blend trees and blend spaces, layers, root motion, events, AnimationTracks, IK, ragdolls, property animation, cutscenes and the animation file formats.
icon: person
samples: [hello-animation, hello-character, hello-cutscene]
---

# Animation

One clip format animates skeletons and properties alike. Each frame, every **Animator** runs its state machine, then any AnimationTracks played over it, then IK and ragdoll poses; the GPU then bends each mesh around its bones (skinning). See [Animation](../learn/animation.md) for the editor side.

## Files

| File | Holds |
|---|---|
| `assets/animations/<name>/<clip>.anim.json` | A clip: bone keys or property tracks, its length, looping and events. A **linked** clip plays a take from an FBX file in `assets/models/`. |
| `assets/animations/<name>/<name>.statemachine.json` | A state machine: parameters, states, transitions, blend trees and layers. |
| `assets/sequences/<name>.sequence.json` | A cutscene: camera cuts, clip, property and sound tracks. |

## State machines

- **Parameters** are bools, numbers and triggers, set from scripts with `Animator:SetBool()`, `Animator:SetNumber()` and `Animator:SetTrigger()` (`Animator:ResetTrigger()` clears one).
- **States** play a clip, a **1D blend tree** (clips mixed by one number, such as speed) or a **2D blend space** (mixed by two, such as direction).
- **Transitions** move between states when their conditions hold, blending over a time; a transition from **Any State** can interrupt whatever's playing.
- **Layers** play over the base layer with a weight and a bone mask: an upper-body layer for waving while walking. Change a weight with `Animator:SetLayerWeight()`.
- **Root motion** lets a clip move the character instead of only posing it.
- **Events** placed on a clip's timeline fire `Animator.AnimationEvent` with their name; `Animator.StateChanged` fires on each state change, and `Animator.CurrentState` says where it is.

## AnimationTracks

`Animator:LoadAnimation()` returns an **AnimationTrack** that plays a clip over the state machine, as in many familiar APIs: `AnimationTrack:Play()`, `AnimationTrack:Stop()`, `AnimationTrack:AdjustSpeed()` and `AnimationTrack:AdjustWeight()`, with fade times. The latest started wins where tracks overlap. `AnimationTrack.Stopped` and `AnimationTrack:GetMarkerReachedSignal()` report progress.

```lua
local animator = workspace.Hero.Animator
local wave = animator:LoadAnimation(workspace.Hero.Wave)
wave:Play(0.2)
wave.Stopped:Connect(function()
	print("Done waving")
end)
```

## Property animation

An **Animation** Instance plays a property clip on its parent: a door's Position, a light's Brightness, a UI element's Size and colour. `Animation:Play()`, `Animation:Pause()`, `Animation:Resume()`, `Animation:Stop()` and `Animation.Completed`. When a player turns on **reduced motion** (`GuiService.ReducedMotion`), UI animations hold still.

## IK and ragdolls

- An **IKControl** turns a chain of bones so its end reaches a target (a hand on a door handle, a head looking at something); `IKControl.Weight` blends it in. Characters' feet are placed on the ground they stand on.
- A **Ragdoll** makes a character go limp under physics and blends it back to animation when it gets up.

## Performance

Skinning runs on the GPU: twenty walking characters cost about 0.4 ms of CPU drawing time. State machines and IK run on the CPU each frame.

> **Known limits.** Animation in glTF files isn't imported; use FBX. Ragdolls can overshoot their joint limits on hard landings.

More: [Animation in the engineering wiki](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Engine-Systems/Animation.md).
