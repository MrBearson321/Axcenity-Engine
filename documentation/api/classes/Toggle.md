---
description: "An on/off switch or checkbox: a click flips its Value."
---

# Toggle

```luau
Instance.new("Toggle")
```

An on/off switch or checkbox: a click flips its Value. Its parts are drawn and styled with ::track, ::thumb and ::check, and :checked styles it while it's on.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
Toggle.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
Toggle.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
Toggle.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
Toggle.Size: UDim2
```

The size relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
Toggle.BackgroundColor3: Color3
```

The background color behind its parts.

#### BackgroundTransparency

```luau
Toggle.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Visible

```luau
Toggle.Visible: boolean
```

If false, it's hidden.

#### Interactable

```luau
Toggle.Interactable: boolean
```

If false, the pointer can't change it, it takes no hover, and CSS :disabled styles it. Scripts can still set its Value.

#### Tooltip

```luau
Toggle.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
Toggle.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
Toggle.Rotation: number
```

Degrees clockwise: it's drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### Value

```luau
Toggle.Value: boolean
```

On (true) or off (false). A click flips it; setting it from a script flips it too. Each change fires ValueChanged. CSS :checked styles it while it's on.

#### Style

```luau
Toggle.Style: string
```

"Switch" (a knob that slides across a track) or "Checkbox" (a box ticked while on). Style its parts with CSS: Toggle::track, ::thumb (a switch's knob) and ::check (a checkbox's tick).

#### Selectable

```luau
Toggle.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
Toggle.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
Toggle.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
Toggle.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
Toggle.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
Toggle.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### ValueChanged

```luau
Toggle.ValueChanged: Signal
```

Fires each time Value changes, from the pointer or a script, with the new Value.

#### MouseEnter

```luau
Toggle.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
Toggle.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

