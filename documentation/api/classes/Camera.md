---
description: "A view of the world."
---

# Camera

```luau
Instance.new("Camera")
```

A view of the world. `workspace.CurrentCamera` (LocalScripts only) is the one the player sees through: its `CameraType` says who moves it, and it turns screen points into rays and back. Camera objects placed in the world are named viewpoints, not drawn: `workspace.CurrentCamera.CFrame = workspace.Overlook.CFrame` cuts to one.

It also has every member of [Instance](Instance.md).

### Properties

#### Position

```luau
Camera.Position: Vector3
```

Where the camera is, in studs.

#### Orientation

```luau
Camera.Orientation: Vector3
```

Which way it looks, in degrees like a Part's: X looks up (positive) and down, Y turns left (positive) and right. Z is ignored.

#### FieldOfView

```luau
Camera.FieldOfView: number
```

How much it sees from top to bottom, in degrees (1 to 120; 50 to start with). Smaller zooms in.

#### CFrame

```luau
Camera.CFrame: CFrame
```

Its Position and Orientation as one CFrame. `workspace.CurrentCamera.CFrame = viewpoint.CFrame` cuts to a viewpoint; TweenService moves it smoothly.

#### LookVector

```luau
Camera.LookVector: Vector3
```

`CurrentCamera` only: the direction it looks in, one stud long. Read-only.

#### CameraType

```luau
Camera.CameraType: EnumCameraType
```

`CurrentCamera` only: who moves it. `Custom` (the default) is the character controller's camera, or the fly camera; `Scriptable` leaves CFrame entirely to scripts; `Follow` and `Track` follow CameraSubject from where the camera was (Follow turning as the subject turns, Track not); `Fixed` stays put. Back to Custom, the controller has it again.

#### CameraSubject

```luau
Camera.CameraSubject: Instance?
```

`CurrentCamera` only: what Follow and Track follow: a Model, a part, or a character controller or agent (its body). Nil follows nothing.

#### Focus

```luau
Camera.Focus: CFrame
```

`CurrentCamera` only: what it's looking at: what a script set, otherwise CameraSubject, otherwise 10 studs in front of it.

#### ViewportSize

```luau
Camera.ViewportSize: Vector2
```

`CurrentCamera` only: the view's size in pixels (the window, or the editor's viewport). Read-only.

### Functions

#### ScreenPointToRay

```luau
Camera:ScreenPointToRay(x: number, y: number, depth: number?): Ray
```

`CurrentCamera` only: the ray from the camera through pixel `x`, `y` (from the view's top left, like `GetMouseLocation`), starting `depth` studs (0 if left out) in front.

#### ViewportPointToRay

```luau
Camera:ViewportPointToRay(x: number, y: number, depth: number?): Ray
```

`CurrentCamera` only: the same as ScreenPointToRay; nothing covers part of the screen.

#### WorldToScreenPoint

```luau
Camera:WorldToScreenPoint(point: Vector3): (Vector3, boolean)
```

`CurrentCamera` only: where `point` shows, in pixels from the view's top left (Z: studs in front of the camera, negative behind it), and whether it's on screen. Put a UI element there with `UDim2.fromOffset(at.X, at.Y)`.

#### WorldToViewportPoint

```luau
Camera:WorldToViewportPoint(point: Vector3): (Vector3, boolean)
```

`CurrentCamera` only: the same as WorldToScreenPoint.

#### Shake

```luau
Camera:Shake(intensity: number, duration: number): ()
```

`CurrentCamera` only: shakes the view for `duration` seconds, fading out, on top of any CameraType. An `intensity` of 1 moves it about a quarter of a stud and turns it a degree; shakes add up.

