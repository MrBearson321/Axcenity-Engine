---
description: "Inside Lighting: moving noise over the picture, as on film, changing every frame."
---

# FilmGrainEffect

```luau
Instance.new("FilmGrainEffect")
```

Inside Lighting: moving noise over the picture, as on film, changing every frame. The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.FilmGrain).

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
FilmGrainEffect.Enabled: boolean
```

Whether it's drawn. Off (or with no FilmGrainEffect in Lighting) it costs nothing.

#### Intensity

```luau
FilmGrainEffect.Intensity: number
```

How strong the noise is, from 0 to 1 (0.3 by default).

#### Size

```luau
FilmGrainEffect.Size: number
```

Each grain's size, in pixels of a view 1080 pixels tall (1 by default), scaled with the view's height and never under one pixel.

