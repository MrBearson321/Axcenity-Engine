---
description: "A box showing its chosen Value."
---

# Dropdown

```luau
Instance.new("Dropdown")
```

A box showing its chosen Value. A click opens a list of its Options, drawn over everything else, which scrolls when long; picking one sets Value, and a click outside closes it. Style the list with ::list and each row with ::option.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
Dropdown.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
Dropdown.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
Dropdown.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
Dropdown.Size: UDim2
```

The size relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
Dropdown.BackgroundColor3: Color3
```

The background color behind its parts.

#### BackgroundTransparency

```luau
Dropdown.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Visible

```luau
Dropdown.Visible: boolean
```

If false, it's hidden.

#### Interactable

```luau
Dropdown.Interactable: boolean
```

If false, the pointer can't change it, it takes no hover, and CSS :disabled styles it. Scripts can still set its Value.

#### Tooltip

```luau
Dropdown.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
Dropdown.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
Dropdown.Rotation: number
```

Degrees clockwise: it's drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### Options

```luau
Dropdown.Options: string
```

The choices, one per line, in the order the list shows them.

#### Value

```luau
Dropdown.Value: string
```

The chosen option's text; empty shows the Placeholder. Picking from the list sets it, and so can a script (to any text). Each change fires ValueChanged.

#### Placeholder

```luau
Dropdown.Placeholder: string
```

Shown faintly while nothing is chosen.

#### TextColor3

```luau
Dropdown.TextColor3: Color3
```

The color of the chosen option's text, unless CSS `color` sets it.

#### TextSize

```luau
Dropdown.TextSize: number
```

The size of its text in pixels, unless CSS `font-size` sets it.

#### Selectable

```luau
Dropdown.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
Dropdown.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
Dropdown.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
Dropdown.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
Dropdown.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
Dropdown.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### ValueChanged

```luau
Dropdown.ValueChanged: Signal
```

Fires each time Value changes, from the pointer or a script, with the new Value.

#### MouseEnter

```luau
Dropdown.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
Dropdown.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

