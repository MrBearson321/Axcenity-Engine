---
description: "The player's game UI settings, from game:GetService(\"GuiService\"), for a settings menu."
---

# GuiService

The player's game UI settings, from `game:GetService("GuiService")`, for a settings menu. They change every HUD and menu (and the UI Designer's artboard) and never the 3D world, and they're separate from the editor's own appearance. Both sides share them, and save slots keep them (as the settings `UI/TextScale`, `UI/HighContrast` and `UI/ReducedMotion`).

### Properties

#### TextScale

```luau
GuiService.TextScale: number
```

How much bigger UI text is drawn than it's authored, from 0.5 to 3; 1 is as authored. Text with TextScaled on still fits its box. Anything else is an error and changes nothing.

#### HighContrast

```luau
GuiService.HighContrast: boolean
```

If true, UI text is drawn black or white (whichever is nearer its colour) and see-through backgrounds are drawn solid.

#### ReducedMotion

```luau
GuiService.ReducedMotion: boolean
```

If true, UI motion that's only for looks happens at once: CSS transitions, tweens of UI elements, and the Animations and AnimationTracks playing on them jump to where they end (a looping one holds its first frame). Events such as Completed and Stopped still fire.

