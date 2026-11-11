---
description: "Inside a part: a picture (Texture) on one of its faces (Face), tinted by Color3 and faded by Transparency."
---

# Decal

```luau
Instance.new("Decal")
```

Inside a part: a picture (Texture) on one of its faces (Face), tinted by Color3 and faded by Transparency. It's stretched across the face, or repeated every StudsPerTileU by StudsPerTileV studs. It shows at every graphics quality.

It also has every member of [Instance](Instance.md).

### Properties

#### Texture

```luau
Decal.Texture: string
```

The picture, a project path such as "assets/textures/logo.png". Its alpha is its shape. Empty shows a plain square.

#### Face

```luau
Decal.Face: EnumNormalId | "Right" | "Top" | "Back" | "Left" | "Bottom" | "Front"
```

The face of its part it's on: `Enum.NormalId.Front` (the default, -Z), `Back`, `Left`, `Right`, `Top` or `Bottom`.

#### Color3

```luau
Decal.Color3: Color3
```

Tints the picture: white shows it as it is.

#### Transparency

```luau
Decal.Transparency: number
```

0 shows the picture as it is, 1 hides it.

#### StudsPerTileU

```luau
Decal.StudsPerTileU: number
```

Studs one copy of the picture covers across the face; it repeats to fill it. 0 (the default) stretches one copy across.

#### StudsPerTileV

```luau
Decal.StudsPerTileV: number
```

Studs one copy of the picture covers down the face; it repeats to fill it. 0 (the default) stretches one copy down.

