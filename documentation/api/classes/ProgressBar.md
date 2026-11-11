---
description: "A bar filled as far as Value is from MinValue to MaxValue, for health, loading and the like."
---

# ProgressBar

```luau
Instance.new("ProgressBar")
```

A bar filled as far as Value is from MinValue to MaxValue, for health, loading and the like. Display only; style the filled part with ::fill.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
ProgressBar.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
ProgressBar.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
ProgressBar.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
ProgressBar.Size: UDim2
```

The size relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
ProgressBar.BackgroundColor3: Color3
```

The background color behind its parts.

#### BackgroundTransparency

```luau
ProgressBar.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Visible

```luau
ProgressBar.Visible: boolean
```

If false, it's hidden.

#### Interactable

```luau
ProgressBar.Interactable: boolean
```

If false, the pointer can't change it, it takes no hover, and CSS :disabled styles it. Scripts can still set its Value.

#### Tooltip

```luau
ProgressBar.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
ProgressBar.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
ProgressBar.Rotation: number
```

Degrees clockwise: it's drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### Value

```luau
ProgressBar.Value: number
```

How far along it is, from MinValue (empty) to MaxValue (full). Setting it fires ValueChanged.

#### MinValue

```luau
ProgressBar.MinValue: number
```

The Value at which it's empty.

#### MaxValue

```luau
ProgressBar.MaxValue: number
```

The Value at which it's full.

#### Orientation

```luau
ProgressBar.Orientation: string
```

"Horizontal" (fills from the left) or "Vertical" (fills from the bottom). Style the filled part with CSS ProgressBar::fill, which can be a linear-gradient.

#### Selectable

```luau
ProgressBar.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
ProgressBar.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
ProgressBar.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
ProgressBar.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
ProgressBar.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
ProgressBar.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### ValueChanged

```luau
ProgressBar.ValueChanged: Signal
```

Fires each time Value changes, from the pointer or a script, with the new Value.

#### MouseEnter

```luau
ProgressBar.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
ProgressBar.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

