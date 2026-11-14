---
description: "Inside a Terrain: an area of the ground, such as a forest or an alpine meadow (procedural worlds PW2)."
---

# Biome

```luau
Instance.new("Biome")
```

Inside a Terrain: an area of the ground, such as a forest or an alpine meadow (procedural worlds PW2). It covers ground where its height, slope and moisture settings hold, with soft edges (later biomes cover earlier ones), and wherever its GroundLayer is painted. Its GroundLayer is laid under the other layers' own rules there, and the ScatterRules inside it place only where it covers, thinning out at its edges. ScatterRules outside every Biome place anywhere.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
Biome.Enabled: boolean
```

If false, the biome covers nothing and the rules inside it place nothing. True by default.

#### GroundLayer

```luau
Biome.GroundLayer: string
```

The name of the TerrainLayer that covers the biome's ground. Painting that layer in the Terrain Studio paints the biome too. Empty for none.

#### AutoEnabled

```luau
Biome.AutoEnabled: boolean
```

If true (the default), the biome covers ground by itself wherever its height, slope and moisture settings hold; if false, only where its GroundLayer is painted.

#### MinHeight

```luau
Biome.MinHeight: number
```

Studs above the terrain's bottom where the biome starts. 0 by default.

#### MaxHeight

```luau
Biome.MaxHeight: number
```

Studs above the terrain's bottom where the biome ends. 10000 by default.

#### HeightFade

```luau
Biome.HeightFade: number
```

Studs over which each height edge fades. 8 by default.

#### MinSlope

```luau
Biome.MinSlope: number
```

Degrees from flat where the biome starts. 0 by default.

#### MaxSlope

```luau
Biome.MaxSlope: number
```

Degrees from flat where the biome ends. 90 by default.

#### SlopeFade

```luau
Biome.SlopeFade: number
```

Degrees over which each slope edge fades. 5 by default.

#### MinMoisture

```luau
Biome.MinMoisture: number
```

0 (dry) to 1 (wet): the driest ground the biome covers. Moisture is noise over the terrain, the same for every biome with the same MoistureScale. 0 by default.

#### MaxMoisture

```luau
Biome.MaxMoisture: number
```

0 to 1: the wettest ground the biome covers. 1 by default.

#### MoistureFade

```luau
Biome.MoistureFade: number
```

How much of the moisture range each moisture edge fades over. 0.1 by default.

#### MoistureScale

```luau
Biome.MoistureScale: number
```

Studs across one dry or wet patch. 400 by default.

#### EdgeNoise

```luau
Biome.EdgeNoise: number
```

0 to 1: breaks the biome's edges up with noise. 0.3 by default.

#### NoiseScale

```luau
Biome.NoiseScale: number
```

Studs across the edge noise's patches. 48 by default.

