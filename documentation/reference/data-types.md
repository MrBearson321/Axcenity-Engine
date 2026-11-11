---
description: Vector3, Vector2, CFrame, Color3, UDim2, Random, TweenInfo, raycast parameters and results, enums and the other value types scripts use.
icon: brackets
samples: [hello-tween, hello-physics]
---

# Data types

Values that aren't Instances: positions, rotations, colours, sizes and settings. They're made with a constructor such as `Vector3.new()` and never change: maths on one makes a new one.

| Type | What it holds | Make one with |
|---|---|---|
| Vector3 | A position, direction or size in studs: X, Y, Z | `Vector3.new(x, y, z)`, `Vector3.zero`, `Vector3.one` |
| Vector2 | A 2D point or size, such as a screen position in pixels | `Vector2.new(x, y)` |
| CFrame | A position and a rotation together | `CFrame.new()`, `CFrame.lookAt()`, `CFrame.Angles()` |
| Color3 | A colour, channels from 0 to 1 | `Color3.new()`, `Color3.fromRGB()`, `Color3.fromHSV()`, `Color3.fromHex()` |
| UDim | One UI measurement: a scale of the parent plus pixels | — |
| UDim2 | A UI position or size: an X and a Y UDim | `UDim2.new()`, `UDim2.fromScale()`, `UDim2.fromOffset()` |
| Random | A seeded random number generator | `Random.new(seed)` |
| TweenInfo | How a tween runs: time, easing, repeats | `TweenInfo.new()` |
| PhysicalProperties | Density, friction and bounciness for a part | `PhysicalProperties.new()` |
| RaycastParams, OverlapParams | Which parts a raycast or area query finds | `RaycastParams.new()`, `OverlapParams.new()` |
| RaycastResult | What a raycast hit: the part, position, normal and distance | Returned by `Workspace:Raycast()` |
| EnumItem | One choice of an enum, such as `Enum.Material.Neon` | `Enum.<Name>.<Item>` |

## Vector3

```lua
local a = Vector3.new(1, 2, 3)
local b = Vector3.new(4, 0, 0)
print(a + b, a * 2, a.Magnitude)
print(a:Dot(b), a:Cross(b), a:Lerp(b, 0.5))
```

`Vector3.Unit` is the same direction with length 1, and `Vector3:FuzzyEq()` compares with a tolerance. `Vector3:Min()`, `Vector3:Max()`, `Vector3:Abs()`, `Vector3:Floor()` and `Vector3:Ceil()` work on each axis.

## CFrame

A CFrame places and turns something in one value. `Part.CFrame` reads and writes a part's Position and Orientation together.

```lua
local part = workspace.Arrow
part.CFrame = CFrame.lookAt(part.Position, Vector3.new(0, 10, 0))
part.CFrame = part.CFrame * CFrame.Angles(0, math.rad(90), 0)
```

`CFrame.LookVector`, `CFrame.RightVector` and `CFrame.UpVector` are its directions; `CFrame:ToWorldSpace()` and `CFrame:ToObjectSpace()` convert between frames, and `CFrame:Inverse()` undoes one.

## Colours

`Color3.fromRGB(255, 128, 0)` takes 0–255 channels, as paint programs show them; `Color3.new()` takes 0–1. `Color3:Lerp()` blends two colours, `Color3:ToHSV()` and `Color3:ToHex()` convert.

## UI sizes

A UDim2 is a scale of the parent plus an offset in pixels, for X and for Y. `UDim2.fromScale(0.5, 0.5)` is half the parent; `UDim2.fromOffset(200, 50)` is 200 by 50 pixels; `UDim2.new(1, -20, 0, 40)` is the parent's width less 20 pixels, 40 pixels tall.

## Random numbers

`Random.new(seed)` gives the same numbers for the same seed on every computer: `Random:NextInteger()`, `Random:NextNumber()`, `Random:NextUnitVector()` and `Random:Shuffle()`. `math.random()` works too.

## Enums

Enums are fixed lists of choices: `Enum.Material`, `Enum.PartType`, `Enum.KeyCode`, `Enum.EasingStyle` and many more. Each item has `EnumItem.Name` and `EnumItem.Value`. Compare items directly: `if input.KeyCode == Enum.KeyCode.Space then`. The API Reference lists every enum and its items.

## Signals and connections

Events are **Signal** values; `Signal:Connect()` returns a **Connection** with `Connection:Disconnect()` and `Connection.Connected`. See [Instances, properties and events](instances.md).
