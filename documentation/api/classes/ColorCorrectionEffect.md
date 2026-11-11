---
description: "Inside Lighting: changes the colors of the finished picture, after tone mapping."
---

# ColorCorrectionEffect

```luau
Instance.new("ColorCorrectionEffect")
```

Inside Lighting: changes the colors of the finished picture, after tone mapping. Brightness, Contrast, Saturation and TintColor come first, then the LookupTable, Posterize and the Palette. Several in Lighting add up (their tints multiply). Inside a GraphicsVolume, those in the volume replace Lighting's while the camera is there.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
ColorCorrectionEffect.Enabled: boolean
```

Whether it changes the picture. Off (or with no ColorCorrectionEffect in Lighting) it costs nothing.

#### Brightness

```luau
ColorCorrectionEffect.Brightness: number
```

Added to every color channel, from -1 (black) to 1 (white). 0 by default.

#### Contrast

```luau
ColorCorrectionEffect.Contrast: number
```

Above 0 spreads colors away from mid-grey; below it squeezes them toward it, down to -1 (flat grey). 0 by default.

#### Saturation

```luau
ColorCorrectionEffect.Saturation: number
```

Above 0 makes colors more vivid; below it greys them, down to -1 (black and white). 0 by default.

#### TintColor

```luau
ColorCorrectionEffect.TintColor: Color3
```

Multiplies every color. White (the default) changes nothing.

#### LookupTable

```luau
ColorCorrectionEffect.LookupTable: string
```

A lookup table in the project every color is looked up in: a .cube file, or a strip image of N squares of N × N pixels side by side (such as 256 × 16). "" for none.

#### Posterize

```luau
ColorCorrectionEffect.Posterize: number
```

How many levels each color channel is cut down to, 2 or more, for a flat, poster look. 0 (the default) leaves them smooth.

#### Palette

```luau
ColorCorrectionEffect.Palette: string
```

An image in the project whose colors (up to 64) every pixel is snapped to the nearest of, for a retro look. "" for none.

