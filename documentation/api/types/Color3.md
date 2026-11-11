---
description: "A color, with each channel from 0 to 1."
---

# Color3

A color, with each channel from 0 to 1.

### Properties

#### R

```luau
Color3.R: number
```

Red, from 0 to 1.

#### G

```luau
Color3.G: number
```

Green, from 0 to 1.

#### B

```luau
Color3.B: number
```

Blue, from 0 to 1.

### Functions

#### Lerp

```luau
Color3:Lerp(goal: Color3, alpha: number): Color3
```

The color `alpha` of the way from this one to `goal` (0 is this one, 1 is `goal`).

#### ToHSV

```luau
Color3:ToHSV(): (number, number, number)
```

Hue, saturation and value, each from 0 to 1, as `Color3.fromHSV` takes them.

#### ToHex

```luau
Color3:ToHex(): string
```

Six lowercase hex digits without a `#`, such as `"ff8000"`.

#### new

```luau
Color3.new(r: number?, g: number?, b: number?): Color3
```

A color from channels between 0 and 1. Left-out channels are 0.

#### fromRGB

```luau
Color3.fromRGB(r: number?, g: number?, b: number?): Color3
```

A color from channels between 0 and 255, as in most paint programs.

```lua
workspace.Sign.Color = Color3.fromRGB(255, 200, 0)
```

#### fromHSV

```luau
Color3.fromHSV(hue: number, saturation: number, value: number): Color3
```

A color from hue, saturation and value, each from 0 to 1. Hue goes round the color wheel from red (0) through green (1/3) and blue (2/3) back to red (1).

#### fromHex

```luau
Color3.fromHex(hex: string): Color3
```

A color from six hex digits such as `"#ff8000"` or three such as `"#f80"`, with or without the `#`.

