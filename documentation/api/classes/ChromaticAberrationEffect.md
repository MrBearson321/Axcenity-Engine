---
description: "Inside Lighting: red and blue drift apart toward the picture's edges, as through a cheap lens."
---

# ChromaticAberrationEffect

```luau
Instance.new("ChromaticAberrationEffect")
```

Inside Lighting: red and blue drift apart toward the picture's edges, as through a cheap lens. The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.ChromaticAberration).

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
ChromaticAberrationEffect.Enabled: boolean
```

Whether it's drawn. Off (or with no ChromaticAberrationEffect in Lighting) it costs nothing.

#### Intensity

```luau
ChromaticAberrationEffect.Intensity: number
```

How far red and blue drift apart toward the picture's edges: 1 parts them by 1% of its width at the left and right edges, and nothing in the middle. 0.5 by default.

