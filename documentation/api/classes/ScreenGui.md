---
description: "The on-screen UI root, usually inside Workspace."
---

# ScreenGui

```luau
Instance.new("ScreenGui")
```

The on-screen UI root, usually inside Workspace.

It also has every member of [Instance](Instance.md).

### Properties

#### Class

```luau
ScreenGui.Class: string
```

Space-separated CSS class names used to style this UI Instance.

#### Id

```luau
ScreenGui.Id: string
```

The CSS identifier used to style this UI Instance.

#### Enabled

```luau
ScreenGui.Enabled: boolean
```

If true, the UI in this ScreenGui is drawn.

#### ScaleMode

```luau
ScreenGui.ScaleMode: string
```

Fixed keeps positions and sizes as authored. Fit lays this ScreenGui out at DesignWidth by DesignHeight, then scales it uniformly so it stays proportional and inside the screen. Scale scales it the same way but lays it out over the whole screen, so elements anchored to an edge stay there.

#### DesignWidth

```luau
ScreenGui.DesignWidth: number
```

The width, in pixels, this ScreenGui is designed at: Fit and Scale scale it from this size. Ignored when ScaleMode is Fixed.

#### DesignHeight

```luau
ScreenGui.DesignHeight: number
```

The height, in pixels, this ScreenGui is designed at: Fit and Scale scale it from this size. Ignored when ScaleMode is Fixed.

#### DisplayOrder

```luau
ScreenGui.DisplayOrder: number
```

Where this ScreenGui is drawn among the others: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree.

#### ScreenInsets

```luau
ScreenGui.ScreenInsets: string
```

"None" lays it out over the whole screen. "DeviceSafeInsets" keeps it inside the device's safe area, clear of a phone's notch and home bar; a desktop screen has none, so it's the whole screen there.

