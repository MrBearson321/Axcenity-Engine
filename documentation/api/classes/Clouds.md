---
description: "Inside Lighting: a layer of cloud high in the sky (800 to 1200 studs up), lit by the sun and sky, whose shadows fall on the world beneath."
---

# Clouds

```luau
Instance.new("Clouds")
```

Inside Lighting: a layer of cloud high in the sky (800 to 1200 studs up), lit by the sun and sky, whose shadows fall on the world beneath. They drift at their own Speed toward their Direction, and workspace.GlobalWind carries them too; their shadows move with them. The player's graphics settings choose how finely it's drawn (UserGameSettings.CloudQuality); at Low it's a flat sheet.

It also has every member of [Instance](Instance.md).

### Properties

#### Cover

```luau
Clouds.Cover: number
```

How much of the sky is cloud, from 0 (a clear sky) to 1 (overcast).

#### Density

```luau
Clouds.Density: number
```

How thick the clouds are, from 0 (none) to 1: thicker clouds are whiter on top, darker underneath, and cast deeper shadows.

#### Color

```luau
Clouds.Color: Color3
```

The clouds' own color, which the sun and sky light. White by default.

#### Speed

```luau
Clouds.Speed: number
```

How fast the clouds drift on their own, in studs a second (20 by default; 0 to hold them still). workspace.GlobalWind adds to it. Their shadows move with them.

#### Direction

```luau
Clouds.Direction: number
```

Which way the clouds drift on their own, in degrees turned from +X (east, 0) toward +Z (south, 90).

