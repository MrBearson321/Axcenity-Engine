---
description: "A box that lays its picture (Texture) onto whatever solid surfaces are inside it, following their shape: bullet holes, puddles, road markings and dirt."
---

# ProjectedDecal

```luau
Instance.new("ProjectedDecal")
```

A box that lays its picture (Texture) onto whatever solid surfaces are inside it, following their shape: bullet holes, puddles, road markings and dirt. The picture faces the box's front (its CFrame's LookVector, -Z), and fades from surfaces turned away from it past half of FadeAngle. With IgnoreCharacters on, characters walking through it stay bare. It isn't drawn at the Low graphics quality, and higher qualities draw more of them, further away. workspace:SpawnDecal places ones that fade out.

It also has every member of [Instance](Instance.md).

### Properties

#### Texture

```luau
ProjectedDecal.Texture: string
```

The picture, a project path such as "assets/textures/hole.png". Its alpha is its shape. Empty shows a plain square.

#### Color3

```luau
ProjectedDecal.Color3: Color3
```

Tints the picture: white shows it as it is.

#### Transparency

```luau
ProjectedDecal.Transparency: number
```

0 shows the picture as it is, 1 hides it. `SpawnDecal`'s decals fade by raising it.

#### Position

```luau
ProjectedDecal.Position: Vector3
```

The middle of its box, in studs.

#### Orientation

```luau
ProjectedDecal.Orientation: Vector3
```

Its turn in degrees (Z, then X, then Y). The picture faces the box's front, -Z (its CFrame's LookVector); the default, 90 around X, faces up, so it lies on the ground below.

#### Size

```luau
ProjectedDecal.Size: Vector3
```

The picture's width and height, then how deep the box reaches (half in front of its middle, half behind): only surfaces inside the box are painted.

#### FadeAngle

```luau
ProjectedDecal.FadeAngle: number
```

Degrees a surface may turn away from the picture's facing before the decal is gone from it: 75 by default. It fades over the second half of that turn, so the sides of what's in the box aren't smeared.

#### IgnoreCharacters

```luau
ProjectedDecal.IgnoreCharacters: boolean
```

On (the default), characters walking through the box stay bare: it paints only the world around them.

