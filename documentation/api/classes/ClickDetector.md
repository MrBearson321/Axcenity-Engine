---
description: "Makes the part it's in clickable."
---

# ClickDetector

```luau
Instance.new("ClickDetector")
```

Makes the part it's in clickable. Pointing at the part from within MaxActivationDistance fires MouseHoverEnter and shows CursorIcon; a left click fires MouseClick and a right click RightMouseClick. Clicks the game UI takes don't count.

It also has every member of [Instance](Instance.md).

### Properties

#### MaxActivationDistance

```luau
ClickDetector.MaxActivationDistance: number
```

Studs from the middle of the player character (or the camera, with no character) to the point clicked. Farther clicks, and pointing from farther, do nothing.

#### CursorIcon

```luau
ClickDetector.CursorIcon: string
```

The cursor over the part, as a CSS cursor name such as "pointer", "crosshair" or "grab". Empty is the pointing hand.

### Events

#### MouseClick

```luau
ClickDetector.MouseClick: Signal
```

Fires with the Player when the left button is pressed and let go on the part, from within MaxActivationDistance.

#### RightMouseClick

```luau
ClickDetector.RightMouseClick: Signal
```

Fires with the Player when the right button is pressed and let go on the part, from within MaxActivationDistance.

#### MouseHoverEnter

```luau
ClickDetector.MouseHoverEnter: Signal
```

Fires with the Player when the mouse starts pointing at the part from within reach.

#### MouseHoverLeave

```luau
ClickDetector.MouseHoverLeave: Signal
```

Fires with the Player when the mouse stops pointing at the part, or goes out of reach.

