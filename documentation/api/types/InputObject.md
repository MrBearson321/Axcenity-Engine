---
description: "One input, as UserInputService's events and ContextActionService's actions get it."
---

# InputObject

One input, as `UserInputService`'s events and `ContextActionService`'s actions get it.

### Properties

#### KeyCode

```luau
InputObject.KeyCode: EnumKeyCode
```

Which key or controller button, such as `Enum.KeyCode.W` or `Enum.KeyCode.ButtonA` (`Thumbstick1` for the left stick), or `Unknown` for the mouse and touches.

#### UserInputType

```luau
InputObject.UserInputType: EnumUserInputType
```

What it came from: `Keyboard`, `MouseButton1` to `MouseButton3`, `MouseMovement`, `MouseWheel`, `Touch`, or a controller (`Gamepad1` to `Gamepad4`).

#### UserInputState

```luau
InputObject.UserInputState: EnumUserInputState
```

`Begin` when it went down, `Change` when it moved, `End` when it came up.

#### Position

```luau
InputObject.Position: Vector3
```

Where the mouse or finger is, in pixels from the window's top left (Z is the wheel's notches), or a stick's position (-1 to 1, up positive).

#### Delta

```luau
InputObject.Delta: Vector3
```

How far it moved: the mouse in pixels, right and down positive, or a stick.

