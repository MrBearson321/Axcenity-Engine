---
description: The UI Instances, layout with UDim2 and anchors, layering, widgets, the CSS subset, text and rich text, keyboard and controller navigation, localisation and accessibility.
icon: cursor
samples: [hello-ui, hello-css, hello-buttons, hello-rich-text, hello-accessibility, hello-strings]
---

# UI

Game UI is Instances, laid out by the engine and styled with CSS. The editor's UI Designer edits the same Instances; see [UI for your game](../learn/ui.md).

## Layout

- A **ScreenGui** in the Workspace is a layer drawn over the game. Several can be on at once; `ScreenGui.DisplayOrder` stacks them and `ScreenGui.Enabled` hides one.
- Elements are placed with **UDim2** `Position` and `Size`: a scale of the parent plus pixels. Pinned right is X scale 1 with a negative offset; centred is scale 0.5; stretched is position scale 0 and size scale 1.
- A ScreenGui's scale mode (**Fixed**, **Fit** or **Scale**) says how a design made at one size fills other screens. `ScreenGui.ScreenInsets` keeps UI out of a phone's notch.
- `ZIndex` orders siblings (CSS `z-index` wins over it); `Rotation` turns an element and its children around its middle.
- CSS flex layout (`display: flex`, `flex-direction`, `gap`, `justify-content`, `align-items`) arranges children automatically.

## Elements and widgets

`Frame`, `TextLabel`, `TextButton`, `ImageLabel`, `TextBox`, `ScrollingFrame` (`ScrollingFrame.CanvasSize`, `ScrollingFrame.CanvasPosition`), `PageLayout` (`PageLayout.CurrentPage`), `Table` (`Table.Columns`) and `BillboardGui` (floats over a part; `BillboardGui.StudsOffset`, `BillboardGui.MaxDistance`).

Widgets draw their own parts and fire **ValueChanged** when their value changes, from the player, a script or a tween:

| Widget | Value |
|---|---|
| `Slider` | A number between MinValue and MaxValue, in Steps. |
| `Toggle` | On or off; a switch or a checkbox. |
| `Dropdown` | One of its Options (one per line). |
| `ProgressBar` | A number shown as a fill; display only. |

## Events

`TextButton.Activated` (a press and release on the button), `TextBox.FocusLost` (typing finished), and `TextButton.MouseEnter` / `TextButton.MouseLeave` fire in **LocalScripts** only. **Interactable** off disables an element; **Tooltip** shows a hint; **HoverSound** and **ClickSound** play without a script.

## CSS

Plain `.css` files anywhere in the project are loaded in path order.

- **Selectors**: type (`TextLabel`), `.class`, `#id`, comma lists, descendants, and `:hover`, `:active`, `:disabled`, `:checked`, `:focus`.
- **Properties**: flex layout, `width`, `height`, `padding`, `margin`, `background-color` and gradients, `color`, `border` (each side), `border-radius`, `border-image`, `box-shadow`, `opacity`, fonts (`font-family`, `font-size`, `font-weight`, `font-style`), `text-decoration`, `text-transform`, `aspect-ratio`, `overflow`, `z-index` and `transition`.
- Widgets' parts have pseudo-elements, such as `Slider::thumb` and `Toggle::track`.
- Properties also has a **Style (CSS)** section that shows and edits the rules that apply to the selected element.

## Text

`TextScaled` fits text to its box. With `RichText` on, tags such as `<b>`, `<i>`, `<u>`, `<s>` and `<font color="#ff8000" size="24">` style parts of a string. Fonts are `.ttf` and `.otf` files in the project.

## Keyboard and controller navigation

Tab, Shift+Tab, the arrow keys, the d-pad and the left stick move focus between **Selectable** elements; Enter or the controller's South button confirms, Escape backs out. The focused element matches `:focus`, with a ring drawn when no rule styles it. These keys are input-map actions, so players can rebind them.

## Localisation

The **string table** (`strings.json`, edited in the UI Designer's **Strings** tab) holds each key's text per language. An element's **TextKey** picks a key; `LocalizationService:GetString()` reads one from a script, and `LocalizationService.Language` is the player's language.

## Accessibility

Players can set a **text scale** (0.5 to 3), **higher contrast** and **reduced motion** (`GuiService.TextScale`, `GuiService.HighContrast`, `GuiService.ReducedMotion`). **AccessibleName** names a control for navigation.

## Built-in UI

The hotbar, leaderboard, chat window, proximity prompts, dialogue box and quest tracker are drawn by the engine over the game UI and styled with CSS by their type names (`Inventory`, `ProximityPrompt`, `DialogueBox`…).

> **Known limits.** No UI on a part's surface (use BillboardGui). The CSS is a subset: no grid layout, animations (`@keyframes`) or media queries.

More: [UI in the engineering wiki](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Engine-Systems/UI.md).
