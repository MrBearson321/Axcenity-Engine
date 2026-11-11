---
description: "A light that shines from a rectangle Width by Height studs, out of its front only, like a window or a softbox: on the side of its parent part its Direction points out of, or anywhere else from its own Position, turned by its Orientation (the editor shows it as an icon you can drag)."
---

# RectLight

```luau
Instance.new("RectLight")
```

A light that shines from a rectangle Width by Height studs, out of its front only, like a window or a softbox: on the side of its parent part its Direction points out of, or anywhere else from its own Position, turned by its Orientation (the editor shows it as an icon you can drag).

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
RectLight.Enabled: boolean
```

If false, the light is off.

#### Color

```luau
RectLight.Color: Color3
```

The light's color.

#### Brightness

```luau
RectLight.Brightness: number
```

How bright the light is.

#### Range

```luau
RectLight.Range: number
```

How far the light reaches, in studs.

#### Width

```luau
RectLight.Width: number
```

How wide the rectangle the light shines from is, in studs.

#### Height

```luau
RectLight.Height: number
```

How tall the rectangle the light shines from is, in studs.

#### Direction

```luau
RectLight.Direction: Vector3
```

The way the rectangle's front faces, relative to the parent part (or to its own Orientation when it isn't in a part).

#### Shadows

```luau
RectLight.Shadows: boolean
```

If true, parts in front of it cast shadows. Only the few lights nearest the camera do, as the graphics settings allow.

#### Position

```luau
RectLight.Position: Vector3
```

Where it is in the world when it isn't inside a part, in studs. Inside a part it works from the part and this isn't used.

#### Orientation

```luau
RectLight.Orientation: Vector3
```

How it's turned when it isn't inside a part, in degrees, as a part's Orientation. Inside a part it turns with the part.

#### Locked

```luau
RectLight.Locked: boolean
```

If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.

