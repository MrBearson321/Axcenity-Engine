---
description: "A UI position or size with scale and pixel-offset components."
---

# UDim2

A UI position or size with scale and pixel-offset components.

### Properties

#### X

```luau
UDim2.X: UDim
```

The horizontal component.

#### Y

```luau
UDim2.Y: UDim
```

The vertical component.

### Functions

#### new

```luau
UDim2.new(xScale: number?, xOffset: number?, yScale: number?, yOffset: number?): UDim2
```

A UI value from scale and pixel offset on each axis. Left-out components are 0.

```lua
local button = script.Parent
-- Scale and offset: full width less 20 pixels, 40 pixels tall.
button.Size = UDim2.new(1, -20, 0, 40)
```

#### fromScale

```luau
UDim2.fromScale(x: number?, y: number?): UDim2
```

A UI value made from scale components, with no pixel offsets.

```lua
local panel = workspace.HUD.Panel
panel.Size = UDim2.fromScale(0.5, 0.25)
panel.Position = UDim2.new(0.5, -100, 0, 20)
```

#### fromOffset

```luau
UDim2.fromOffset(x: number?, y: number?): UDim2
```

A UI value made from pixel offsets, with no scale components.

