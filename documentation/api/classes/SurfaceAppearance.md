---
description: "Texture maps for a Part, or for one material of a MeshPart (named by MaterialName, in place of the file's own)."
---

# SurfaceAppearance

```luau
Instance.new("SurfaceAppearance")
```

Texture maps for a Part, or for one material of a MeshPart (named by MaterialName, in place of the file's own). Delete it to go back to the part's Color and Material, or the file's maps.

It also has every member of [Instance](Instance.md).

### Properties

#### MaterialName

```luau
SurfaceAppearance.MaterialName: string
```

On a MeshPart, the model's material whose maps this replaces. Unused on a Part. Import Model sets it.

#### Color

```luau
SurfaceAppearance.Color: Color3
```

Multiplies the material's color (and its ColorMap).

#### ColorMap

```luau
SurfaceAppearance.ColorMap: string
```

The base color texture, a file inside the project folder such as "assets/textures/wood.png". Empty for none.

#### NormalMap

```luau
SurfaceAppearance.NormalMap: string
```

The normal map's file, for bumps and grooves. Empty for none.

#### MetalnessMap

```luau
SurfaceAppearance.MetalnessMap: string
```

The metalness map's file (read from MetalnessChannel). Empty for the model's own metalness.

#### RoughnessMap

```luau
SurfaceAppearance.RoughnessMap: string
```

The roughness map's file (read from RoughnessChannel). Empty for the model's own roughness.

#### EmissiveMap

```luau
SurfaceAppearance.EmissiveMap: string
```

The file of colors the surface glows in. Empty for the model's own glow.

#### AmbientOcclusionMap

```luau
SurfaceAppearance.AmbientOcclusionMap: string
```

The ambient occlusion map's file: dark where ambient light can't reach. Empty for none.

#### OpacityMap

```luau
SurfaceAppearance.OpacityMap: string
```

The opacity map's file (read from OpacityChannel): the surface is cut out where it's below half. Empty for the color map's alpha.

#### SpecularMap

```luau
SurfaceAppearance.SpecularMap: string
```

How strongly non-metal parts shine, for older materials: mid grey is the usual. Empty for none.

#### NormalStrength

```luau
SurfaceAppearance.NormalStrength: number
```

How far the normal map tips the surface: 1 as made, 0 flat.

#### NormalMapDirectX

```luau
SurfaceAppearance.NormalMapDirectX: boolean
```

True for normal maps made for DirectX, whose green points the other way.

#### MetalnessChannel

```luau
SurfaceAppearance.MetalnessChannel: EnumTextureChannel | "Red" | "Green" | "Blue" | "Alpha"
```

Which channel of MetalnessMap holds metalness, for packed textures.

#### RoughnessChannel

```luau
SurfaceAppearance.RoughnessChannel: EnumTextureChannel | "Red" | "Green" | "Blue" | "Alpha"
```

Which channel of RoughnessMap holds roughness, for packed textures.

#### InvertRoughness

```luau
SurfaceAppearance.InvertRoughness: boolean
```

True if RoughnessMap is a glossiness or smoothness map: white is smooth.

#### AmbientOcclusionChannel

```luau
SurfaceAppearance.AmbientOcclusionChannel: EnumTextureChannel | "Red" | "Green" | "Blue" | "Alpha"
```

Which channel of AmbientOcclusionMap holds it, for packed textures.

#### AmbientOcclusionStrength

```luau
SurfaceAppearance.AmbientOcclusionStrength: number
```

How much AmbientOcclusionMap darkens, from 0 to 1.

#### OpacityChannel

```luau
SurfaceAppearance.OpacityChannel: EnumTextureChannel | "Red" | "Green" | "Blue" | "Alpha"
```

Which channel of OpacityMap holds opacity.

#### InvertOpacity

```luau
SurfaceAppearance.InvertOpacity: boolean
```

True if OpacityMap is white where the surface is see-through.

#### EmissiveStrength

```luau
SurfaceAppearance.EmissiveStrength: number
```

Multiplies the glow.

#### Tiling

```luau
SurfaceAppearance.Tiling: Vector2
```

How many times as often the maps repeat, across and down: `Vector2.new(2, 2)` repeats them twice as often. On a Part, on top of its TileSize.

#### Offset

```luau
SurfaceAppearance.Offset: Vector2
```

Shifts the maps across and down, in copies of the texture: `Vector2.new(0.5, 0)` moves them half a copy across.

