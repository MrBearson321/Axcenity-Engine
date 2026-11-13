---
description: "A landscape: a heightfield shaped in the editor's Terrain Studio, drawn with its TerrainLayers and colliding as its ground."
---

# Terrain

```luau
Instance.new("Terrain")
```

A landscape: a heightfield shaped in the editor's Terrain Studio, drawn with its TerrainLayers and colliding as its ground. Position is the middle of its footprint at the bottom of its height range; Size is its width, height range and length.

It also has every member of [Instance](Instance.md).

### Properties

#### Position

```luau
Terrain.Position: Vector3
```

The middle of the terrain's bottom: its lowest possible ground.

#### Size

```luau
Terrain.Size: Vector3
```

Width (X), height range (Y) and length (Z) in studs. Heights are stored as parts of the height range, so changing Y stretches the hills.

#### CastShadow

```luau
Terrain.CastShadow: boolean
```

Whether the terrain casts shadows.

#### CanCollide

```luau
Terrain.CanCollide: boolean
```

Whether parts and characters stand on the terrain.

#### DataFolder

```luau
Terrain.DataFolder: string
```

The project folder its heights and painted materials are saved in, such as "assets/terrain/Terrain". The editor picks it.

#### Water

```luau
Terrain.Water: boolean
```

Floods the terrain's hollows up to WaterLevel with water that has waves, foam and reflections.

#### WaterLevel

```luau
Terrain.WaterLevel: number
```

The world Y of the water's surface at rest. Ground below it is under water.

#### WaterColor

```luau
Terrain.WaterColor: Color3
```

The color deep water shows.

#### WaterClarity

```luau
Terrain.WaterClarity: number
```

How many studs of water light crosses before only about a third of what's under it still shows. Bigger is clearer.

#### WaveHeight

```luau
Terrain.WaveHeight: number
```

How high the biggest wave rises above the water's surface at rest, in studs.

#### WaveLength

```luau
Terrain.WaveLength: number
```

How long the biggest wave is from crest to crest, in studs.

#### WaveSpeed

```luau
Terrain.WaveSpeed: number
```

How fast the waves roll, as a share of how fast real waves of their length would. 0 stills them.

#### WaveDirection

```luau
Terrain.WaveDirection: number
```

Which way the waves roll, in degrees: 0 toward +X, 90 toward +Z.

