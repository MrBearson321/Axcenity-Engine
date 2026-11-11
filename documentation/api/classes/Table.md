---
description: "Lays its children out as equal cells, left to right then top to bottom."
---

# Table

```luau
Instance.new("Table")
```

Lays its children out as equal cells, left to right then top to bottom. Columns is how many cells share a row.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
Table.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
Table.Id: string
```

The CSS identifier used to style this UI Instance.

#### Position

```luau
Table.Position: UDim2
```

The top-left position relative to the parent, in scale and pixel offsets.

#### Size

```luau
Table.Size: UDim2
```

The size relative to the parent, in scale and pixel offsets.

#### BackgroundColor3

```luau
Table.BackgroundColor3: Color3
```

The table's background color.

#### BackgroundTransparency

```luau
Table.BackgroundTransparency: number
```

The background opacity: 0 is opaque and 1 is transparent.

#### Visible

```luau
Table.Visible: boolean
```

If false, this table is hidden.

#### Interactable

```luau
Table.Interactable: boolean
```

If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.

#### Tooltip

```luau
Table.Tooltip: string
```

Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.

#### ZIndex

```luau
Table.ZIndex: number
```

Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.

#### Rotation

```luau
Table.Rotation: number
```

Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.

#### Columns

```luau
Table.Columns: number
```

How many cells sit in one row. Cells are the table's children, left to right then top to bottom, each the same size.

#### Selectable

```luau
Table.Selectable: boolean
```

If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.

#### NextSelectionUp

```luau
Table.NextSelectionUp: string
```

The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionDown

```luau
Table.NextSelectionDown: string
```

The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionLeft

```luau
Table.NextSelectionLeft: string
```

The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.

#### NextSelectionRight

```luau
Table.NextSelectionRight: string
```

The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.

#### AccessibleName

```luau
Table.AccessibleName: string
```

The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.

### Events

#### MouseEnter

```luau
Table.MouseEnter: Signal
```

Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.

#### MouseLeave

```luau
Table.MouseLeave: Signal
```

Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.

