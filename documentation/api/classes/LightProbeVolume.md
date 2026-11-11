---
description: "Inside a Part: a grid of light probes filling the part's box, baked in the editor (Lighting > Bake Probes) with the light from the sun, lamps and sky and what it bounces off."
---

# LightProbeVolume

```luau
Instance.new("LightProbeVolume")
```

Inside a Part: a grid of light probes filling the part's box, baked in the editor (Lighting > Bake Probes) with the light from the sun, lamps and sky and what it bounces off. Everything in the box, still or moving, is lit by the probes around it in place of the sky's even light, so a part beside a red wall picks up its red. Overlapping volumes blend, the smaller one winning, and each fades into what's around it over its BlendDistance. Changing the world doesn't change a bake until it's baked again; the Explorer warns when it's out of date.

It also has every member of [Instance](Instance.md).

### Properties

#### Bake

```luau
LightProbeVolume.Bake: string
```

The bake the editor saved for it (Lighting > Bake Probes), such as "assets/probes/Hall.lightprobes.json". Empty, or a file that's missing, leaves the sky's light instead.

#### Spacing

```luau
LightProbeVolume.Spacing: number
```

About how many studs apart its probes stand along each side of the part (at most 16 probes a side). Bake again after changing it.

#### BlendDistance

```luau
LightProbeVolume.BlendDistance: number
```

How far inside the part's faces, in studs, its light fades in from what's around it.

