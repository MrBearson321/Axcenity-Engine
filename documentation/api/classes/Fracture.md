---
description: "Breaks the Part or MeshPart it's in into pieces."
---

# Fracture

```luau
Instance.new("Fracture")
```

Breaks the Part or MeshPart it's in into pieces. Generate (in its Properties) cuts the part into Pieces convex pieces by Seed and Pattern and saves them as `<name>.fracture.glb`. During Play, something hitting the part faster than Threshold, or `Shatter`, replaces the part with its pieces, which fly from the point and fade out after DebrisLifetime.

It also has every member of [Instance](Instance.md).

### Properties

#### Pieces

```luau
Fracture.Pieces: number
```

How many pieces Generate (in its Properties) cuts the part into.

#### Seed

```luau
Fracture.Seed: number
```

The same Seed always makes the same pieces; change it for a different break.

#### Pattern

```luau
Fracture.Pattern: EnumFracturePattern | "Uniform" | "Impact"
```

`Enum.FracturePattern.Uniform` makes pieces of about one size; `Impact` makes more, smaller pieces toward the part's middle, as where something struck it.

#### InteriorMaterial

```luau
Fracture.InteriorMaterial: EnumMaterial | "Plastic" | "SmoothPlastic" | "Wood" | "WoodPlanks" | "Marble" | "Granite" | "Concrete" | "Slate" | "Brick" | "Fabric" | "Sand" | "Metal" | "Glass" | "Neon" | "Ice" | "Rubber"
```

What the faces the cuts make inside the part look like, in the part's color.

#### Threshold

```luau
Fracture.Threshold: number
```

How fast, in studs a second, something must hit the part (the two closing at the point they touch) to shatter it. 0 or less: only `Shatter` breaks it.

#### DebrisLifetime

```luau
Fracture.DebrisLifetime: number
```

Seconds the pieces last once the part shatters. They fade out over the last second and are destroyed. 0 keeps them.

#### PiecesId

```luau
Fracture.PiecesId: string
```

The pieces' file, which Generate writes: `<name>.fracture.glb` next to the MeshPart's model, or in `assets/models` for a Part. Empty until it's generated; the part can't shatter until then.

### Events

#### Shattered

```luau
Fracture.Shattered: Signal
```

Fires once when the part is replaced by its pieces, hit hard enough or by `Shatter`. The handler gets no arguments; the part has left the game (its Parent is nil) and the pieces are in a Model named after it.

### Functions

#### Shatter

```luau
Fracture:Shatter(point: Vector3?, force: number?): ()
```

Replaces the part with its pieces before the next physics step, flying away from `point` (the part's middle if nil) at up to `force` ÷ the part's mass studs a second: that much for the nearest pieces, half for the farthest. Fires Shattered. Does nothing until Generate has made the pieces.

