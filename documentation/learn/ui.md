---
description: Screen UI with ScreenGuis, frames, text, buttons and widgets, the UI Designer, styling with CSS, event graphs, and UI in the 3D world.
icon: cursor
samples: [hello-ui, hello-buttons, hello-css, hello-ui-graph, hello-settings]
---

# UI for your game

Game UI is made of Instances, like the rest of the world: a **ScreenGui** is a layer drawn over the game, and the elements inside it are laid out in it.

## The elements

| Element | Use it for |
|---|---|
| ScreenGui | A layer: a HUD, a menu, a pause screen. Turn one on and off with **Enabled**; **DisplayOrder** puts layers in front of each other. |
| Frame | A box that holds other elements. |
| TextLabel | Text. |
| TextButton | A button with text, an image or both. Called **Button** on the toolbar. |
| ImageLabel | A picture. |
| TextBox | A field the player types in. |
| ScrollingFrame | A box that scrolls its contents. |
| PageLayout | Pages (Frames) shown one at a time; set **CurrentPage** to switch. |
| Table | Children laid out in equal cells, **Columns** wide. |
| Slider, Toggle, Dropdown, ProgressBar | Ready-made widgets for settings and health bars. |
| BillboardGui | UI that floats over a part in the 3D world, such as a name tag. |

## The UI Designer

The **UI Designer** tab (beside the Scene) shows the selected ScreenGui on an artboard the size of a screen; with none it offers **New ScreenGui**. Drag elements from the palette, move and resize them with handles, and set **anchors** so they stay in a corner or stretch as the screen changes size. **Preview** shows the layout on other screens, from 4:3 desktops to phones. Drag guides out of the rulers to line things up, and select several elements to move or align them together. The inspector shows the selected element's position, its anchor and its style, and the **States** editor sets how a button looks when hovered, pressed or disabled.

## Styling with CSS

Give elements a **Class** or an **Id**, then style them from `.css` files in the project folder: colours, fonts, padding, borders, rounded corners, flex layout, transitions and `:hover` states. One stylesheet styles every screen, so changing a colour changes it everywhere. Properties also has a **Style (CSS)** section that shows and edits the rules that apply to the selected element. The **hello-css** sample shows the supported rules.

```css
.primary {
  background-color: #2f6fed;
  color: white;
  border-radius: 6px;
  padding: 8px 16px;
}
.primary:hover {
  background-color: #4b84f0;
}
```

## Making buttons do something

ScreenGuis live in the Workspace. Use a **LocalScript**, since UI runs on each player's side:

```lua
local gui = workspace.MainMenu
local start = gui.Buttons.Start
start.Activated:Connect(function()
	gui.Enabled = false
end)
```

Or, without code, switch the UI Designer to **Event Graph**: wire a button's click to nodes that switch pages, change text or set values. The graph compiles to an ordinary LocalScript. The **hello-ui-graph** sample has one.

## Text for every language

Write text through the **string table** so the game can be translated: the UI Designer's **Strings** tab lists the languages and keys (such as `menu.play`) with a text for each language, and an element's **TextKey** picks a key. The **hello-strings** sample shows how.

## Built-in game UI

Some UI comes with the engine and is styled with the same CSS: the hotbar for Tools, the leaderboard, chat, proximity prompts, dialogue boxes and the quest tracker. **Insert ▸ UI ▸ Health bar** adds a health bar that follows a Health with no script.

> **Known limits.** There's no UI drawn onto a part's surface yet: in the 3D world, use a BillboardGui. The CSS covers the common rules, not all of CSS.
