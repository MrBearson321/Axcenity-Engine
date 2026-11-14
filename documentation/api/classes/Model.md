---
description: "Groups the parts of one thing, such as a car or a house."
---

# Model

```luau
Instance.new("Model")
```

Groups the parts of one thing, such as a car or a house.

It also has every member of [Instance](Instance.md).

### Properties

#### ModelStreamingMode

```luau
Model.ModelStreamingMode: EnumModelStreamingMode | "Default" | "Atomic" | "Persistent"
```

How the Model streams. Default: a Model with no Models inside loads and unloads whole, and one with Models inside streams each piece on its own. Atomic: always whole, with the Models inside. Persistent: always on every device.

#### Locked

```luau
Model.Locked: boolean
```

If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.

### Functions

#### ApplyEffect

```luau
Model:ApplyEffect(effect: Effect): Effect?
```

Scripts only: puts a copy of the Effect `effect` on this Model and returns it, or adds a stack to the copy already here (up to its MaxStacks) and starts its Duration over. An instant Effect changes at once and returns nil.

#### GetPivot

```luau
Model:GetPivot(): CFrame
```

Where it turns and moves from: a part's own CFrame, or for a Model the centre of the box around its parts, unturned.

```lua
local house = workspace.House
local where = house:GetPivot()
house:PivotTo(where * CFrame.new(0, 0, -10)) -- move it 10 studs forward
```

#### PivotTo

```luau
Model:PivotTo(target: CFrame): ()
```

Moves and turns it so its pivot is at `target`. A Model's parts all move together and keep their places relative to each other.

```lua
local house = workspace.House
house:PivotTo(CFrame.new(0, 0, 50) * CFrame.Angles(0, math.rad(90), 0))
```

