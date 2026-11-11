---
description: "A clickable UI element."
---

# TextButton

```luau
Instance.new("TextButton")
```

A clickable UI element. Text and Image can each be empty; both filled shows an icon and a label. Fires Activated when clicked. The old name TextButton still loads.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
TextButton.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
TextButton.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
TextButton.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
TextButton.Size: UDim2
```

The size relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
TextButton.BackgroundColor3: Color3
```

The button's background color.

#### BackgroundTransparency

```luau
TextButton.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Text

```luau
TextButton.Text: string
```

The text to display. With a TextKey, this is the source text, shown when the string table has no value for it.

#### TextKey

```luau
TextButton.TextKey: string
```

A key in the project's string table, such as "menu.play": shows its value in LocalizationService.Language instead of Text. Empty, or a key or language with no value, shows Text.

#### TextColor3

```luau
TextButton.TextColor3: Color3
```

The text color.

#### TextSize

```luau
TextButton.TextSize: number
```

The text size in pixels.

#### TextTransparency

```luau
TextButton.TextTransparency: number
```

How see-through its text is, from 0 (solid) to 1 (invisible); its outline and shadows fade with it. The background and picture keep their own transparency, and CSS opacity still fades the whole element.

#### TextScaled

```luau
TextButton.TextScaled: boolean
```

If true, the text is drawn at the largest size up to TextSize (or CSS font-size) that fits its box.

#### RichText

```luau
TextButton.RichText: boolean
```

If true, tags in Text style parts of it: <b>, <i>, <u>, <s>, <font color size face weight>, <stroke color thickness>, <uc> and <br/>. Write &lt; and &amp; for < and &.

#### Image

```luau
TextButton.Image: string
```

The project-relative path of an icon. Empty shows no picture. With Text, the icon sits beside the label.

#### ImageTransparency

```luau
TextButton.ImageTransparency: number
```

The icon opacity: 0 is opaque and 1 is transparent.

#### Visible

```luau
TextButton.Visible: boolean
```

If false, this button is hidden and cannot be activated.

#### Interactable

```luau
TextButton.Interactable: boolean
```

If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.

#### Tooltip

```luau
TextButton.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
TextButton.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
TextButton.Rotation: number
```

Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### HoverSound

```luau
TextButton.HoverSound: string
```

A sound file (a path in the project) played once when the pointer comes onto the button. Empty plays nothing.

#### ClickSound

```luau
TextButton.ClickSound: string
```

A sound file (a path in the project) played once when the button is clicked. Empty plays nothing.

#### Selectable

```luau
TextButton.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
TextButton.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
TextButton.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
TextButton.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
TextButton.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
TextButton.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### Activated

```luau
TextButton.Activated: Signal
```

Fires when the player clicks or taps this button. The handler gets no arguments; LocalScripts only.

```lua
-- In a LocalScript
local menu = workspace.MainMenu
menu.Buttons.Play.Activated:Connect(function()
	menu.Enabled = false
end)
```

#### MouseEnter

```luau
TextButton.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
TextButton.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

