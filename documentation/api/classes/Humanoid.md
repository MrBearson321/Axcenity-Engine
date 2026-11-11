---
description: "A Roblox-familiar front for a character, in its Model."
---

# Humanoid

```luau
Instance.new("Humanoid")
```

A Roblox-familiar front for a character, in its Model. It finds the Model's character controller, Health, Inventory and Animator, making each one that's missing during Play, and its WalkSpeed, RunSpeed, JumpHeight, Sit, PlatformStand, AutoRotate and MoveDirection are the controller's, and Health, MaxHealth, DisplayName and HealthDisplayDistance the Health's: there's one value, so the pieces work the same with or without it. It stores only BreakJointsOnDeath and RagdollOnDeath. Its Animator plays its Idle, Walk, Run, Jump and Fall states, if it has them.

It also has every member of [Instance](Instance.md).

### Properties

#### BreakJointsOnDeath

```luau
Humanoid.BreakJointsOnDeath: boolean
```

If true, dying turns off the WeldConstraints and joints in the character (not its Ragdolls') and unanchors its parts, so they fall apart.

#### RagdollOnDeath

```luau
Humanoid.RagdollOnDeath: boolean
```

If true, dying turns on every Ragdoll in the character.

#### WalkSpeed

```luau
Humanoid.WalkSpeed: number
```

Its controller's WalkSpeed.

#### RunSpeed

```luau
Humanoid.RunSpeed: number
```

Its controller's RunSpeed.

#### JumpHeight

```luau
Humanoid.JumpHeight: number
```

Its controller's JumpHeight.

#### Sit

```luau
Humanoid.Sit: boolean
```

Its controller's Sit.

#### PlatformStand

```luau
Humanoid.PlatformStand: boolean
```

Its controller's PlatformStand.

#### AutoRotate

```luau
Humanoid.AutoRotate: boolean
```

Its controller's AutoRotate.

#### MoveDirection

```luau
Humanoid.MoveDirection: Vector3
```

Its controller's MoveDirection.

#### Health

```luau
Humanoid.Health: number
```

Its Health's Health; setting it fires HealthChanged (and Died at 0).

#### MaxHealth

```luau
Humanoid.MaxHealth: number
```

Its Health's MaxHealth.

#### DisplayName

```luau
Humanoid.DisplayName: string
```

Its Health's DisplayName.

#### HealthDisplayDistance

```luau
Humanoid.HealthDisplayDistance: number
```

Its Health's HealthDisplayDistance.

### Events

#### MoveToFinished

```luau
Humanoid.MoveToFinished: Signal
```

Fires when a MoveTo ends. The handler gets true if the character got there, or false if it couldn't (no path on the navigation mesh, 8 seconds passed, or it died).

#### StateChanged

```luau
Humanoid.StateChanged: Signal
```

Fires when the character's state changes. The handler gets the old and new `Enum.HumanoidStateType`: Running, Jumping, Freefall, Landed (for the frame it lands), Seated, Ragdoll or Dead. Swimming is reserved.

#### Died

```luau
Humanoid.Died: Signal
```

Fires once when the character's Health reaches 0, after the Health's own Died and StateChanged to Dead. By then the controller has stopped and RagdollOnDeath and BreakJointsOnDeath are done.

### Functions

#### MoveTo

```luau
Humanoid:MoveTo(position: Vector3, part: (Part | MeshPart)?): ()
```

Walks the character to `position` along a path on the navigation mesh, by its controller's MoveDirection, and fires MoveToFinished(true) when it's there (within half a stud across the ground), or MoveToFinished(false) if there's no path, 8 seconds pass or it dies. With `part`, it walks to the place `position` was relative to the part, as the part moves. A new MoveTo, or Move, replaces it.

#### Move

```luau
Humanoid:Move(direction: Vector3, relativeToCamera: boolean?): ()
```

Sets the controller's MoveDirection to `direction` across the ground (relative to where the camera looks if `relativeToCamera`, on the client), and stops any MoveTo. A zero vector stops.

#### Jump

```luau
Humanoid:Jump(): ()
```

Jumps once from the ground (the controller's Jump).

#### TakeDamage

```luau
Humanoid:TakeDamage(amount: number, source: Instance?): ()
```

Its Health's TakeDamage(amount, source).

```lua
local lava = workspace.Lava
lava.Touched:Connect(function(other)
	local humanoid = other.Parent:FindFirstChildOfClass("Humanoid")
	if humanoid then
		humanoid:TakeDamage(10)
	end
end)
```

#### EquipTool

```luau
Humanoid:EquipTool(tool: Tool): ()
```

Puts `tool` in the character's Inventory (an error if it's full) and in its hand.

#### UnequipTools

```luau
Humanoid:UnequipTools(): ()
```

Puts away the Tool in hand (its Inventory's UnequipAll).

#### GetState

```luau
Humanoid:GetState(): EnumItem
```

The character's state, as of the last frame (Running before the first).

#### ChangeState

```luau
Humanoid:ChangeState(state: EnumItem): ()
```

Asks for a state: Running stands it up (off a Seat, out of PlatformStand, its Ragdolls off), Jumping jumps, Ragdoll turns its Ragdolls on, and Dead sets its Health to 0. The others follow the character and are an error.

