---
description: "A UI element that displays text."
---

# TextLabel

```luau
Instance.new("TextLabel")
```

A UI element that displays text.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
TextLabel.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
TextLabel.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
TextLabel.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
TextLabel.Size: UDim2
```

The size relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
TextLabel.BackgroundColor3: Color3
```

The label's background color.

#### BackgroundTransparency

```luau
TextLabel.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Text

```luau
TextLabel.Text: string
```

The text to display. With a TextKey, this is the source text, shown when the string table has no value for it.

#### TextKey

```luau
TextLabel.TextKey: string
```

A key in the project's string table, such as "menu.play": shows its value in LocalizationService.Language instead of Text. Empty, or a key or language with no value, shows Text.

#### TextColor3

```luau
TextLabel.TextColor3: Color3
```

The text color.

#### TextSize

```luau
TextLabel.TextSize: number
```

The text size in pixels.

#### TextTransparency

```luau
TextLabel.TextTransparency: number
```

How see-through its text is, from 0 (solid) to 1 (invisible); its outline and shadows fade with it. The background and picture keep their own transparency, and CSS opacity still fades the whole element.

#### TextScaled

```luau
TextLabel.TextScaled: boolean
```

If true, the text is drawn at the largest size up to TextSize (or CSS font-size) that fits its box.

#### RichText

```luau
TextLabel.RichText: boolean
```

If true, tags in Text style parts of it: <b>, <i>, <u>, <s>, <font color size face weight>, <stroke color thickness>, <uc> and <br/>. Write &lt; and &amp; for < and &.

#### Visible

```luau
TextLabel.Visible: boolean
```

If false, this label is hidden.

#### Interactable

```luau
TextLabel.Interactable: boolean
```

If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.

#### Tooltip

```luau
TextLabel.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
TextLabel.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
TextLabel.Rotation: number
```

Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### Selectable

```luau
TextLabel.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
TextLabel.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
TextLabel.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
TextLabel.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
TextLabel.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
TextLabel.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### MouseEnter

```luau
TextLabel.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
TextLabel.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

