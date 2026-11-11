---
description: "A track with a thumb: drag it, or click the track, to pick a number from MinValue to MaxValue (in Steps)."
---

# Slider

```luau
Instance.new("Slider")
```

A track with a thumb: drag it, or click the track, to pick a number from MinValue to MaxValue (in Steps). Its parts are drawn, not child Instances, and styled with ::track, ::fill and ::thumb.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
Slider.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
Slider.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
Slider.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
Slider.Size: UDim2
```

The size relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
Slider.BackgroundColor3: Color3
```

The background color behind its parts.

#### BackgroundTransparency

```luau
Slider.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Visible

```luau
Slider.Visible: boolean
```

If false, it's hidden.

#### Interactable

```luau
Slider.Interactable: boolean
```

If false, the pointer can't change it, it takes no hover, and CSS :disabled styles it. Scripts can still set its Value.

#### Tooltip

```luau
Slider.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
Slider.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
Slider.Rotation: number
```

Degrees clockwise: it's drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### Value

```luau
Slider.Value: number
```

The number picked, from MinValue to MaxValue. Dragging the thumb or clicking the track sets it; setting it from a script moves the thumb. Each change fires ValueChanged.

#### MinValue

```luau
Slider.MinValue: number
```

The number at the track's start (its left, or its bottom when Vertical).

#### MaxValue

```luau
Slider.MaxValue: number
```

The number at the track's end (its right, or its top when Vertical).

#### Step

```luau
Slider.Step: number
```

Dragging picks numbers this far apart, counted from MinValue. 0 picks any number.

#### Orientation

```luau
Slider.Orientation: string
```

"Horizontal" (MinValue at the left) or "Vertical" (MinValue at the bottom). Style its parts with CSS: Slider::track, ::fill (the track up to the thumb) and ::thumb.

#### Selectable

```luau
Slider.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
Slider.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
Slider.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
Slider.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
Slider.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
Slider.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### ValueChanged

```luau
Slider.ValueChanged: Signal
```

Fires each time Value changes, from the pointer or a script, with the new Value.

#### MouseEnter

```luau
Slider.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
Slider.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

