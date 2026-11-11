---
description: "A point and a direction on the part it's parented to, for things that need a spot on a part."
---

# Attachment

```luau
Instance.new("Attachment")
```

A point and a direction on the part it's parented to, for things that need a spot on a part. `Position` and `Orientation` (and `CFrame`) are relative to the part, so it moves with it; `WorldCFrame`, `WorldPosition` and `WorldOrientation` are where it is in the world. `Axis` and `SecondaryAxis` are the ways its X and Y point on the part (`WorldAxis` and `WorldSecondaryAxis` in the world). Not drawn in the game; the editor shows it as a small gizmo.

It also has every member of [Instance](Instance.md).

### Properties

#### Position

```luau
Attachment.Position: Vector3
```

Where it is, in studs from the middle of its part, along the part's own axes.

#### Orientation

```luau
Attachment.Orientation: Vector3
```

How it's turned relative to its part, in degrees like a Part's Orientation.

#### CFrame

```luau
Attachment.CFrame: CFrame
```

Its Position and Orientation on its part as one CFrame. Setting it sets both.

#### WorldCFrame

```luau
Attachment.WorldCFrame: CFrame
```

Where it is in the world: its part's CFrame times its own. Setting it moves it on its part.

#### WorldPosition

```luau
Attachment.WorldPosition: Vector3
```

Where it is in the world. Setting it moves it on its part.

#### WorldOrientation

```luau
Attachment.WorldOrientation: Vector3
```

How it's turned in the world, in degrees. Setting it turns it on its part.

#### Axis

```luau
Attachment.Axis: Vector3
```

The way its X points, relative to its part. Setting it turns the attachment, keeping SecondaryAxis as close as it can.

#### SecondaryAxis

```luau
Attachment.SecondaryAxis: Vector3
```

The way its Y points, relative to its part. Setting it turns the attachment around its Axis.

#### WorldAxis

```luau
Attachment.WorldAxis: Vector3
```

The way its X points in the world. Read-only.

#### WorldSecondaryAxis

```luau
Attachment.WorldSecondaryAxis: Vector3
```

The way its Y points in the world. Read-only.

