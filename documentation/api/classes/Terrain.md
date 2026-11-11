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

