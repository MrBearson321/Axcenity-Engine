---
description: "Inside a Terrain: one material it's painted with (a tint, texture maps and tiling), and an optional rule that lays it down by height and slope."
---

# TerrainLayer

```luau
Instance.new("TerrainLayer")
```

Inside a Terrain: one material it's painted with (a tint, texture maps and tiling), and an optional rule that lays it down by height and slope.

It also has every member of [Instance](Instance.md).

### Properties

#### Color

```luau
TerrainLayer.Color: Color3
```

Tints the layer's color map, or is its color without one.

#### ColorMap

```luau
TerrainLayer.ColorMap: string
```

The layer's color texture, relative to the project folder. Empty means none.

#### NormalMap

```luau
TerrainLayer.NormalMap: string
```

The layer's normal map. Empty means none.

#### RoughnessMap

```luau
TerrainLayer.RoughnessMap: string
```

The layer's roughness map. Empty means none.

#### MetalnessMap

```luau
TerrainLayer.MetalnessMap: string
```

The layer's metalness map. Empty means none.

#### AmbientOcclusionMap

```luau
TerrainLayer.AmbientOcclusionMap: string
```

The layer's ambient occlusion map. Empty means none.

#### HeightMap

```luau
TerrainLayer.HeightMap: string
```

The layer's height map: light is high. Where layers meet, the higher one shows through.

#### Metalness

```luau
TerrainLayer.Metalness: number
```

0 to 1: how metallic the layer is, without a metalness map.

#### Roughness

```luau
TerrainLayer.Roughness: number
```

0 to 1: how rough the layer is, without a roughness map.

#### NormalStrength

```luau
TerrainLayer.NormalStrength: number
```

Scales the normal map's bumps.

#### NormalMapDirectX

```luau
TerrainLayer.NormalMapDirectX: boolean
```

True for a DirectX-style normal map, whose green points down.

#### RoughnessChannel

```luau
TerrainLayer.RoughnessChannel: EnumTextureChannel | "Red" | "Green" | "Blue" | "Alpha"
```

Which channel of RoughnessMap holds the roughness.

#### InvertRoughness

```luau
TerrainLayer.InvertRoughness: boolean
```

Reads RoughnessMap as smoothness (1 minus the value).

#### MetalnessChannel

```luau
TerrainLayer.MetalnessChannel: EnumTextureChannel | "Red" | "Green" | "Blue" | "Alpha"
```

Which channel of MetalnessMap holds the metalness.

#### AmbientOcclusionChannel

```luau
TerrainLayer.AmbientOcclusionChannel: EnumTextureChannel | "Red" | "Green" | "Blue" | "Alpha"
```

Which channel of AmbientOcclusionMap holds the occlusion.

#### TileSize

```luau
TerrainLayer.TileSize: number
```

Studs one copy of the textures covers.

#### TextureRotation

```luau
TerrainLayer.TextureRotation: number
```

Turns the textures, in degrees.

#### Triplanar

```luau
TerrainLayer.Triplanar: boolean
```

Projects the textures from the sides too, so cliffs don't stretch them.

#### AntiTiling

```luau
TerrainLayer.AntiTiling: number
```

0 to 1: mixes in a larger, turned copy of the textures so repeats show less.

#### HeightBlend

```luau
TerrainLayer.HeightBlend: number
```

0 to 1: how sharply layers meet where they have height maps.

#### AutoEnabled

```luau
TerrainLayer.AutoEnabled: boolean
```

Whether the layer lays itself down by its height and slope rule.

#### MinHeight

```luau
TerrainLayer.MinHeight: number
```

The rule's lowest height, in studs above the terrain's bottom.

#### MaxHeight

```luau
TerrainLayer.MaxHeight: number
```

The rule's highest height, in studs above the terrain's bottom.

#### HeightFade

```luau
TerrainLayer.HeightFade: number
```

Studs over which the rule fades at its heights.

#### MinSlope

```luau
TerrainLayer.MinSlope: number
```

The rule's gentlest slope, in degrees from flat.

#### MaxSlope

```luau
TerrainLayer.MaxSlope: number
```

The rule's steepest slope, in degrees from flat.

#### SlopeFade

```luau
TerrainLayer.SlopeFade: number
```

Degrees over which the rule fades at its slopes.

#### RuleStrength

```luau
TerrainLayer.RuleStrength: number
```

0 to 1: how fully the layer covers where its rule holds.

#### RuleNoise

```luau
TerrainLayer.RuleNoise: number
```

0 to 1: breaks the rule's edges up with noise.

#### NoiseScale

```luau
TerrainLayer.NoiseScale: number
```

Studs across the rule's noise patches.

