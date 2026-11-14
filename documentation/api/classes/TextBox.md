---
description: "A box the player can type in."
---

# TextBox

```luau
Instance.new("TextBox")
```

A box the player can type in. MultiLine off (the default): Enter finishes. MultiLine on: Enter is a new line and clicking away finishes. Fires FocusLost.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
TextBox.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
TextBox.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
TextBox.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
TextBox.Size: UDim2
```

The size relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
TextBox.BackgroundColor3: Color3
```

The box's background color.

#### BackgroundTransparency

```luau
TextBox.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Text

```luau
TextBox.Text: string
```

The text the player has typed.

#### Placeholder

```luau
TextBox.Placeholder: string
```

Grey text shown while Text is empty.

#### TextColor3

```luau
TextBox.TextColor3: Color3
```

The text color.

#### TextSize

```luau
TextBox.TextSize: number
```

The text size in pixels.

#### TextTransparency

```luau
TextBox.TextTransparency: number
```

How see-through its text is, from 0 (solid) to 1 (invisible); its outline and shadows fade with it. The background and picture keep their own transparency, and CSS opacity still fades the whole element.

#### TextScaled

```luau
TextBox.TextScaled: boolean
```

If true, the text is drawn at the largest size up to TextSize (or CSS font-size) that fits its box.

#### RichText

```luau
TextBox.RichText: boolean
```

If true, tags in Text style parts of it: <b>, <i>, <u>, <s>, <font color size face weight>, <stroke color thickness>, <uc> and <br/>. Write &lt; and &amp; for < and &.

#### MultiLine

```luau
TextBox.MultiLine: boolean
```

If false, Enter finishes typing. If true, Enter starts a new line and clicking away finishes typing.

#### Visible

```luau
TextBox.Visible: boolean
```

If false, this text box is hidden.

#### Interactable

```luau
TextBox.Interactable: boolean
```

If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.

#### Tooltip

```luau
TextBox.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
TextBox.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
TextBox.Rotation: number
```

Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### Selectable

```luau
TextBox.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
TextBox.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
TextBox.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
TextBox.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
TextBox.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
TextBox.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### FocusLost

```luau
TextBox.FocusLost: Signal
```

Fires when the player finishes typing: Enter on a one-line box, or clicking away (or Escape) on any box. The handler gets no arguments; LocalScripts only.

```lua
local box = script.Parent
box.FocusLost:Connect(function(enterPressed)
	if enterPressed then
		print("You typed:", box.Text)
	end
end)
```

#### MouseEnter

```luau
TextBox.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
TextBox.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

