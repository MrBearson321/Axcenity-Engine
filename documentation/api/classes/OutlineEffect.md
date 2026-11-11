---
description: "Inside Lighting: lines drawn along the edges of everything solid, around each silhouette and along creases where a surface turns sharply, found on screen from what's drawn, so no part needs any setup."
---

# OutlineEffect

```luau
Instance.new("OutlineEffect")
```

Inside Lighting: lines drawn along the edges of everything solid, around each silhouette and along creases where a surface turns sharply, found on screen from what's drawn, so no part needs any setup. A material file's outline draws a line around its own parts instead (see docs/rendering/stylized.md).

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
OutlineEffect.Enabled: boolean
```

Whether the lines are drawn. Off (or with no OutlineEffect in Lighting) they cost nothing.

#### Color

```luau
OutlineEffect.Color: Color3
```

The lines' color. Black by default.

#### Thickness

```luau
OutlineEffect.Thickness: number
```

How thick the lines are, in pixels of a view 1080 pixels tall (1 by default), scaled with the view's height and never under one pixel.

