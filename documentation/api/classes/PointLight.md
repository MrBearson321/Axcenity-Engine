---
description: "A light that shines in all directions from just above its parent part."
---

# PointLight

```luau
Instance.new("PointLight")
```

A light that shines in all directions from just above its parent part. Anywhere else (in the Workspace, a Model or a Folder) it needs no part: it shines from its own Position, as in Unreal, and the editor shows it as an icon you can drag.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
PointLight.Enabled: boolean
```

If false, the light is off.

#### Color

```luau
PointLight.Color: Color3
```

The light's color.

#### Brightness

```luau
PointLight.Brightness: number
```

How bright the light is.

#### Range

```luau
PointLight.Range: number
```

How far the light reaches, in studs.

#### Shadows

```luau
PointLight.Shadows: boolean
```

If true, parts around the light cast shadows on every side of it. Only the few point lights nearest the camera do, as the graphics settings allow.

#### Position

```luau
PointLight.Position: Vector3
```

Where it is in the world when it isn't inside a part, in studs. Inside a part it works from the part and this isn't used.

#### Orientation

```luau
PointLight.Orientation: Vector3
```

How it's turned when it isn't inside a part, in degrees, as a part's Orientation. Inside a part it turns with the part.

#### Locked

```luau
PointLight.Locked: boolean
```

If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.

