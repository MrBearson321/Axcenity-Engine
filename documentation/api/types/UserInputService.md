---
description: "The player's keyboard, mouse, touch screen and game controllers, and the input map's actions, from game:GetService(\"UserInputService\")."
---

# UserInputService

The player's keyboard, mouse, touch screen and game controllers, and the input map's actions, from `game:GetService("UserInputService")`. LocalScripts only.

### Properties

#### InputBegan

```luau
UserInputService.InputBegan: Signal
```

Fires when a key, mouse button, controller button or finger goes down. The handler gets an InputObject and `gameProcessedEvent`, true when the game's UI took it (a click on a button, typing in a TextBox, a key that moved focus): `function(input, gameProcessedEvent) ... end`.

```lua
-- In a LocalScript
local UserInputService = game:GetService("UserInputService")
UserInputService.InputBegan:Connect(function(input)
	if input.KeyCode == Enum.KeyCode.E then
		print("E pressed")
	end
end)
```

#### InputChanged

```luau
UserInputService.InputChanged: Signal
```

Fires when the mouse moves, the wheel turns, a finger moves or a controller's stick moves, with the same arguments as InputBegan. `input.Delta` is how far it moved; for the wheel, `input.Position.Z` is the notches.

#### InputEnded

```luau
UserInputService.InputEnded: Signal
```

Fires when a key, button or finger comes back up, with the same arguments as InputBegan.

#### TouchStarted

```luau
UserInputService.TouchStarted: Signal
```

Fires when a finger touches the screen, with the same arguments as InputBegan (before InputBegan).

#### TouchMoved

```luau
UserInputService.TouchMoved: Signal
```

Fires when a finger moves on the screen, with the same arguments as InputBegan.

#### TouchEnded

```luau
UserInputService.TouchEnded: Signal
```

Fires when a finger leaves the screen, with the same arguments as InputBegan.

#### GamepadConnected

```luau
UserInputService.GamepadConnected: Signal
```

Fires when a controller is plugged in (or first used). The handler gets which one: `function(gamepad) ... end`, such as `Enum.UserInputType.Gamepad1`.

#### GamepadDisconnected

```luau
UserInputService.GamepadDisconnected: Signal
```

Fires when a controller is unplugged, with which one, as GamepadConnected.

#### MouseBehavior

```luau
UserInputService.MouseBehavior: EnumMouseBehavior
```

What the cursor does: `Default` moves freely; `LockCenter` holds it, hidden, in the middle of the window, and `LockCurrentPosition` where it is. While it's held, `GetMouseDelta` still says how far the mouse moved, for a camera.

#### MouseIconEnabled

```luau
UserInputService.MouseIconEnabled: boolean
```

False hides the cursor over the game.

### Functions

#### IsKeyDown

```luau
UserInputService:IsKeyDown(key: EnumKeyCode): boolean
```

True while keyboard key `key` is held down, such as `Enum.KeyCode.W`.

```lua
local UserInputService = game:GetService("UserInputService")
game:GetService("RunService").Heartbeat:Connect(function()
	if UserInputService:IsKeyDown(Enum.KeyCode.LeftShift) then
		print("Sprinting")
	end
end)
```

#### IsMouseButtonPressed

```luau
UserInputService:IsMouseButtonPressed(button: EnumUserInputType): boolean
```

True while mouse button `button` is held, such as `Enum.UserInputType.MouseButton1`.

#### GetMouseLocation

```luau
UserInputService:GetMouseLocation(): Vector2
```

Where the mouse is, in pixels from the window's top left.

#### GetMouseDelta

```luau
UserInputService:GetMouseDelta(): Vector2
```

How far the mouse moved this frame, in pixels, right and down positive. It works while the cursor is locked (`MouseBehavior`).

#### GetConnectedGamepads

```luau
UserInputService:GetConnectedGamepads(): {EnumUserInputType}
```

The controllers plugged in, such as `{Enum.UserInputType.Gamepad1}`.

#### IsGamepadButtonDown

```luau
UserInputService:IsGamepadButtonDown(gamepad: EnumUserInputType, button: EnumKeyCode): boolean
```

True while controller `gamepad` holds `button`, such as `Enum.KeyCode.ButtonA`.

#### GetGamepadState

```luau
UserInputService:GetGamepadState(gamepad: EnumUserInputType): {InputObject}
```

An InputObject for each of controller `gamepad`'s buttons and sticks: `UserInputState` Begin while held, and the sticks' `Position`.

#### IsActionPressed

```luau
UserInputService:IsActionPressed(name: string): boolean
```

True while something bound to action `name` in the input map is held, such as `"Jump"`. Input the UI took doesn't count.

#### GetActionAxis

```luau
UserInputService:GetActionAxis(name: string): Vector2
```

Where axis action `name` points, such as `"Move"`: right and up positive, at most 1 long for keys and sticks (the mouse adds its pixels on top).

#### GetActionInputs

```luau
UserInputService:GetActionInputs(name: string): {string}
```

The names of what action `name` is bound to now, such as `{"Space", "ButtonA"}`, for a controls menu.

#### RebindAction

```luau
UserInputService:RebindAction(name: string, inputs: {any}): ()
```

Binds action `name` to `inputs` instead: `Enum.KeyCode` items, mouse buttons (`Enum.UserInputType.MouseButton1`) or an axis's name such as `"W,A,S,D"`. The built-in controllers use it at once. It's kept as the SaveService setting `Input/<name>`, so it's saved in slots and comes back when one loads.

