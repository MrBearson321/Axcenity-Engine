---
description: "Moves the character model it is parented to just like a ThirdPersonController, with the camera at the character's eyes during Play, looking where the player looks."
---

# FirstPersonController

```luau
Instance.new("FirstPersonController")
```

Moves the character model it is parented to just like a ThirdPersonController, with the camera at the character's eyes during Play, looking where the player looks. A character has one controller or the other.

It also has every member of [Instance](Instance.md).

### Properties

#### FootPlacement

```luau
FirstPersonController.FootPlacement: boolean
```

If true, a skinned character plants each foot on what's under it on slopes and stairs, and lowers its hips to match, on each device. It needs an Animator and a skeleton with `LeftFoot` and `RightFoot` bones.

#### Movement

```luau
FirstPersonController.Movement: string
```

"Walk", "Run" or "Crouch", or any other name for a custom mode: the controller stops moving the character, and scripts and blocks move it. Set "" to clear a custom mode; the controller puts back the built-in mode it had. Set and read from a script or the set-property and property blocks.

#### WalkSpeed

```luau
FirstPersonController.WalkSpeed: number
```

How fast Movement "Walk" moves, in studs a second, on flat ground.

#### RunSpeed

```luau
FirstPersonController.RunSpeed: number
```

How fast Movement "Run" moves, in studs a second, on flat ground.

#### JumpHeight

```luau
FirstPersonController.JumpHeight: number
```

How high a jump rises, in studs. Space jumps during Play.

#### Jump

```luau
FirstPersonController.Jump: boolean
```

Set true to jump once if the character is on the ground. The controller sets it back to false on its next step, so a jump in the air does nothing.

#### MaxSlopeAngle

```luau
FirstPersonController.MaxSlopeAngle: number
```

The steepest slope the character walks up, in degrees from flat. A steeper slope stops it.

#### MaxStepHeight

```luau
FirstPersonController.MaxStepHeight: number
```

The tallest ledge the character steps onto without jumping, in studs. A taller one stops it.

#### Crouch

```luau
FirstPersonController.Crouch: boolean
```

Set true to crouch and false to stand. Holding C crouches too. The character stays crouched while something overhead is in the way of standing.

#### CrouchHeight

```luau
FirstPersonController.CrouchHeight: number
```

How tall the character is while crouched, in studs. Its parts squash toward its feet, and the eyes drop as far as its top.

#### CrouchSpeedScale

```luau
FirstPersonController.CrouchSpeedScale: number
```

0 to 1: how much of WalkSpeed or RunSpeed the character keeps while crouched.

#### Sit

```luau
FirstPersonController.Sit: boolean
```

True while the character sits on a Seat (its Movement reads "Seated"). Set it false to stand the character up a little in front of the seat, as Jump does. Setting it true does nothing; use `seat:Sit(controller)`.

#### MoveDirection

```luau
FirstPersonController.MoveDirection: Vector3
```

Where the character walks while no move key is held for it, across the ground (Y is ignored); a length under 1 walks that much slower. A controller that doesn't receive input (an NPC's) walks it too. Humanoid:Move and MoveTo set it.

#### PlatformStand

```luau
FirstPersonController.PlatformStand: boolean
```

If true, the controller neither moves nor jumps the character; it stays where it is until this is false again.

#### AutoRotate

```luau
FirstPersonController.AutoRotate: boolean
```

If true, the character turns about the middle of its body to face the way it walks (a first-person one, the way the camera looks). It's taken to face -Z when Play starts.

#### EyeHeight

```luau
FirstPersonController.EyeHeight: number
```

How far above the character's feet its eyes are, in studs. The camera sits there during Play.

### Events

#### Seated

```luau
FirstPersonController.Seated: Signal
```

Fires when the character sits down (true and the Seat) or stands up (false and the Seat it left).

#### Landed

```luau
FirstPersonController.Landed: Signal
```

Fires when the character lands, with how fast it was falling in studs a second. A Health in its Model with FallDamage on takes its damage first.

