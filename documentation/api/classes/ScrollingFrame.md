---
description: "A Frame that scrolls: its children sit on a canvas (CanvasSize) bigger than the frame, moved by CanvasPosition with the wheel, its scroll bars or a drag."
---

# ScrollingFrame

```luau
Instance.new("ScrollingFrame")
```

A Frame that scrolls: its children sit on a canvas (CanvasSize) bigger than the frame, moved by CanvasPosition with the wheel, its scroll bars or a drag. Only what's inside the frame shows and takes clicks.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
ScrollingFrame.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
ScrollingFrame.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
ScrollingFrame.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
ScrollingFrame.Size: UDim2
```

The size of the window onto the canvas, relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
ScrollingFrame.BackgroundColor3: Color3
```

The frame's background color.

#### BackgroundTransparency

```luau
ScrollingFrame.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Visible

```luau
ScrollingFrame.Visible: boolean
```

If false, this frame and its children are hidden.

#### Interactable

```luau
ScrollingFrame.Interactable: boolean
```

If false, it takes no hover or clicks and CSS :disabled styles it.

#### Tooltip

```luau
ScrollingFrame.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
ScrollingFrame.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
ScrollingFrame.Rotation: number
```

Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### CanvasSize

```luau
ScrollingFrame.CanvasSize: UDim2
```

The area the children are laid out on: scale of the frame's own size plus pixels. It's never smaller than the frame. The default, 2 scale down, is twice as tall as the frame.

#### CanvasPosition

```luau
ScrollingFrame.CanvasPosition: Vector2
```

How far the canvas is scrolled, in pixels from its top left: `Vector2.new(0, 120)` is 120 pixels down. Kept between 0 and as far as the canvas goes. The wheel, the scroll bars and dragging the content set it.

#### ScrollingDirection

```luau
ScrollingFrame.ScrollingDirection: string
```

Which ways it scrolls: "X" (across), "Y" (up and down) or "XY" (both).

#### ScrollBarThickness

```luau
ScrollingFrame.ScrollBarThickness: number
```

How thick the scroll bars are, in pixels; 0 hides them. Style them with CSS: ScrollingFrame::scrollbar for the track and ::thumb for the part you drag.

#### AutomaticCanvasSize

```luau
ScrollingFrame.AutomaticCanvasSize: string
```

"None", "X", "Y" or "XY": the canvas grows that way to fit the children, past CanvasSize.

#### Selectable

```luau
ScrollingFrame.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
ScrollingFrame.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
ScrollingFrame.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
ScrollingFrame.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
ScrollingFrame.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
ScrollingFrame.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### MouseEnter

```luau
ScrollingFrame.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
ScrollingFrame.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

