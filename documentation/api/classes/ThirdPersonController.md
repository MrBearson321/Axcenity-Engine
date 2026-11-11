---
description: "Walks, runs, jumps and crouches the character model it is parented to, kinematically, up slopes and steps within its limits, and owns the camera during Play: behind the character, zoomed with the mouse wheel."
---

# ThirdPersonController

```luau
Instance.new("ThirdPersonController")
```

Walks, runs, jumps and crouches the character model it is parented to, kinematically, up slopes and steps within its limits, and owns the camera during Play: behind the character, zoomed with the mouse wheel. Movement is "Walk", "Run", "Crouch" or a custom mode that scripts move. It is not a physics body: tuning its speeds and limits does not change how other parts fall.

It also has every member of [Instance](Instance.md).

### Properties

#### FootPlacement

```luau
ThirdPersonController.FootPlacement: boolean
```

If true, a skinned character plants each foot on what's under it on slopes and stairs, and lowers its hips to match, on each device. It needs an Animator and a skeleton with `LeftFoot` and `RightFoot` bones.

#### Movement

```luau
ThirdPersonController.Movement: string
```

"Walk", "Run" or "Crouch", or any other name for a custom mode: the controller stops moving the character, and scripts and blocks move it. Set "" to clear a custom mode; the controller puts back the built-in mode it had. Set and read from a script or the set-property and property blocks.

#### WalkSpeed

```luau
ThirdPersonController.WalkSpeed: number
```

How fast Movement "Walk" moves, in studs a second, on flat ground.

#### RunSpeed

```luau
ThirdPersonController.RunSpeed: number
```

How fast Movement "Run" moves, in studs a second, on flat ground.

#### JumpHeight

```luau
ThirdPersonController.JumpHeight: number
```

How high a jump rises, in studs. Space jumps during Play.

#### Jump

```luau
ThirdPersonController.Jump: boolean
```

Set true to jump once if the character is on the ground. The controller sets it back to false on its next step, so a jump in the air does nothing.

#### MaxSlopeAngle

```luau
ThirdPersonController.MaxSlopeAngle: number
```

The steepest slope the character walks up, in degrees from flat. A steeper slope stops it.

#### MaxStepHeight

```luau
ThirdPersonController.MaxStepHeight: number
```

The tallest ledge the character steps onto without jumping, in studs. A taller one stops it.

#### Crouch

```luau
ThirdPersonController.Crouch: boolean
```

Set true to crouch and false to stand. Holding C crouches too. The character stays crouched while something overhead is in the way of standing.

#### CrouchHeight

```luau
ThirdPersonController.CrouchHeight: number
```

How tall the character is while crouched, in studs. Its parts squash toward its feet, and the camera drops as far as its top.

#### CrouchSpeedScale

```luau
ThirdPersonController.CrouchSpeedScale: number
```

0 to 1: how much of WalkSpeed or RunSpeed the character keeps while crouched.

#### Sit

```luau
ThirdPersonController.Sit: boolean
```

True while the character sits on a Seat (its Movement reads "Seated"). Set it false to stand the character up a little in front of the seat, as Jump does. Setting it true does nothing; use `seat:Sit(controller)`.

#### MoveDirection

```luau
ThirdPersonController.MoveDirection: Vector3
```

Where the character walks while no move key is held for it, across the ground (Y is ignored); a length under 1 walks that much slower. A controller that doesn't receive input (an NPC's) walks it too. Humanoid:Move and MoveTo set it.

#### PlatformStand

```luau
ThirdPersonController.PlatformStand: boolean
```

If true, the controller neither moves nor jumps the character; it stays where it is until this is false again.

#### AutoRotate

```luau
ThirdPersonController.AutoRotate: boolean
```

If true, the character turns about the middle of its body to face the way it walks (a first-person one, the way the camera looks). It's taken to face -Z when Play starts.

#### CameraDistance

```luau
ThirdPersonController.CameraDistance: number
```

How far behind the character the camera sits during Play, in studs. The mouse wheel zooms it between CameraMinZoomDistance and CameraMaxZoomDistance and keeps the zoom here.

#### CameraMinZoomDistance

```luau
ThirdPersonController.CameraMinZoomDistance: number
```

The closest the mouse wheel zooms the camera, in studs (at least 0.5). Set it to CameraMaxZoomDistance to lock the distance.

#### CameraMaxZoomDistance

```luau
ThirdPersonController.CameraMaxZoomDistance: number
```

The farthest the mouse wheel zooms the camera out, in studs.

#### CameraHeight

```luau
ThirdPersonController.CameraHeight: number
```

How far above the character's feet the camera looks, in studs.

### Events

#### Seated

```luau
ThirdPersonController.Seated: Signal
```

Fires when the character sits down (true and the Seat) or stands up (false and the Seat it left).

#### Landed

```luau
ThirdPersonController.Landed: Signal
```

Fires when the character lands, with how fast it was falling in studs a second. A Health in its Model with FallDamage on takes its damage first.

