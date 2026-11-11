---
description: "A UI element that displays an image from the project."
---

# ImageLabel

```luau
Instance.new("ImageLabel")
```

A UI element that displays an image from the project. It is not clickable.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
ImageLabel.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
ImageLabel.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
ImageLabel.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
ImageLabel.Size: UDim2
```

The size relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
ImageLabel.BackgroundColor3: Color3
```

The image label's background color.

#### BackgroundTransparency

```luau
ImageLabel.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Image

```luau
ImageLabel.Image: string
```

The project-relative path of the image to display.

#### ImageTransparency

```luau
ImageLabel.ImageTransparency: number
```

The image opacity: 0 is opaque and 1 is transparent.

#### Visible

```luau
ImageLabel.Visible: boolean
```

If false, this image is hidden.

#### Interactable

```luau
ImageLabel.Interactable: boolean
```

If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.

#### Tooltip

```luau
ImageLabel.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
ImageLabel.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
ImageLabel.Rotation: number
```

Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### Selectable

```luau
ImageLabel.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
ImageLabel.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
ImageLabel.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
ImageLabel.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
ImageLabel.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
ImageLabel.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### MouseEnter

```luau
ImageLabel.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
ImageLabel.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

