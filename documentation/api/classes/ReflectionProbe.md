---
description: "Inside a Part: shiny surfaces in the part's box reflect the world as the editor baked it from the part's middle (Lighting > Bake Probes), lined up with the box's walls, in place of the sky."
---

# ReflectionProbe

```luau
Instance.new("ReflectionProbe")
```

Inside a Part: shiny surfaces in the part's box reflect the world as the editor baked it from the part's middle (Lighting > Bake Probes), lined up with the box's walls, in place of the sky. Overlapping probes blend, the smaller one winning, and each fades into what's around it over its BlendDistance. Changing the world doesn't change a bake until it's baked again. Drawn at Medium reflection quality and above.

It also has every member of [Instance](Instance.md).

### Properties

#### Bake

```luau
ReflectionProbe.Bake: string
```

The bake the editor saved for it (Lighting > Bake Probes), such as "assets/probes/Hall.probe.hdr". Empty, or a file that's missing, reflects the sky instead.

#### BlendDistance

```luau
ReflectionProbe.BlendDistance: number
```

How far inside the part's faces, in studs, its reflection fades in from what's around it.

