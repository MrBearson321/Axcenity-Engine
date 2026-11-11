---
description: "How a part weighs, grips and bounces."
---

# PhysicalProperties

How a part weighs, grips and bounces. Where two parts touch, each coefficient is the average of theirs, weighted by their weights.

### Properties

#### Density

```luau
PhysicalProperties.Density: number
```

Mass per cubic stud, from 0.0001 to 100. Plastic is 1, Metal 7.85.

#### Friction

```luau
PhysicalProperties.Friction: number
```

From 0 (slides like ice) to 2 (grips).

#### Elasticity

```luau
PhysicalProperties.Elasticity: number
```

How much of its speed a bounce keeps, from 0 (none) to 1 (all).

#### FrictionWeight

```luau
PhysicalProperties.FrictionWeight: number
```

How much its friction counts against the other part's, from 0 to 100.

#### ElasticityWeight

```luau
PhysicalProperties.ElasticityWeight: number
```

How much its elasticity counts against the other part's, from 0 to 100.

### Functions

#### new

```luau
PhysicalProperties.new(densityOrMaterial: number | EnumMaterial, friction: number?, elasticity: number?, frictionWeight: number?, elasticityWeight: number?): PhysicalProperties
```

A material's properties (`PhysicalProperties.new(Enum.Material.Ice)`), or your own: `PhysicalProperties.new(density, friction, elasticity, frictionWeight, elasticityWeight)`. The weights are 1 when left out; numbers out of range are brought into it.

