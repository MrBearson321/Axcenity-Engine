# Game UI

Read before working in the `ui` crate. This covers in-game UI; the editor's own UI is `egui` and lives in `editor`.

## Model

- UI elements are **Instances**: `ScreenGui`, `Frame`, `TextLabel`, `TextButton` (toolbar name Button), `ImageLabel`, `TextBox`, `PageLayout`, `Table`, `BillboardGui`.
- A `ScreenGui` is a layer. Several can be enabled at once (a HUD and a menu). A page is a `Frame` inside a `PageLayout`, named by `Name`. `CurrentPage` is that name; empty shows the first child Frame. A script that sets a name with no matching Frame is rejected and the last page stays. Hide a whole layer with `ScreenGui.Enabled`.
- `TextButton` has `Text` and `Image`. Empty means unused; both filled is an icon plus a label. `ImageLabel` stays a picture that is not clicked. Saved worlds still use the class name `TextButton`.
- `TextBox` is one line unless `MultiLine` is on. Enter finishes a one-line box. In a multi-line box Enter is a newline and clicking away (or Escape) finishes. Either finish fires `FocusLost` in LocalScripts. Placeholder text shows while `Text` is empty.
- `Table` lays its children out as equal cells, left to right then top to bottom, with an 8 pixel gap. `Columns` (default 2) is how many share a row. A child's CSS width does not change the column size.
- `BillboardGui` is parented to a Part or MeshPart. It is drawn with the screen UI, at the parent position plus `StudsOffset` (world studs, not rotated). `Size` is scale of the viewport plus pixels. `MaxDistance` 0 means no limit.
- Each styled element has `Class` (space-separated class names) and `Id` properties, used by CSS selectors. `BillboardGui` has them so descendant rules can target it.
- Scripts and blocks change UI through normal properties and events (`Text`, `Visible`, `Activated`, `CurrentPage`, `FocusLost`). The Looks block "show page" sets `PageLayout.CurrentPage`.

## Styling: a CSS subset (plain CSS, not SCSS)

- Plain `.css` files anywhere in the project folder are loaded in path order.
- Selectors: type (`TextLabel`), `.class`, `#id`, comma lists, and descendant selectors. Specificity and later-source order follow CSS cascade rules.
- Properties: flex layout (`display`, `flex-direction`, `justify-content`, `align-items`, `gap`, `flex-grow`), `width`/`height` (px and %), `padding`, `margin`, `background-color`, `color`, `border`, `border-radius`, `font-family`, `font-size`, `opacity`, CSS variables (`--name` / `var()` with fallbacks), `background-image: linear-gradient(...)` (vertical; a leading `to …` direction is ignored) and `box-shadow` (drawn as a larger, softer rectangle behind the element). Corners, borders, padding, list layout, shadows and gradients stay CSS. There are no UICorner, UIPadding, UIListLayout, UIShadow or UIGradient objects.
- `Visible = false` removes an element from flex layout and hit testing. `:hover` follows the topmost visible element under the pointer; `:active` applies to the visible button being held while the pointer remains over it. Unsupported properties produce a warning in Output, never a crash.
- Specificity and cascade follow normal CSS rules for the supported subset.

## Layout and drawing

- Layout is computed by **`taffy`** (frame order step 5). Text is shaped with `cosmic-text` or `glyphon`.
- `ui` resolves styles and produces UI draw data, and `render` draws it as an overlay. The editor viewport previews UI while editing as well as during Play; edit-mode scripts do not run.
- `TextButton.Activated` fires on release after a press that began and ended over the same visible button. Only LocalScripts receive it. `ImageLabel.Image` and `TextButton.Image` are project-relative asset paths and are drawn. `font-family` maps `sans-serif` and `Montserrat` to Montserrat, and `monospace` and `JetBrains Mono` to JetBrains Mono; both fonts ship with the engine.

## Editing UI in the editor

- The toolbar's UI group inserts a ScreenGui, Frame, TextLabel (Text), TextButton (Button), ImageLabel (Image), TextBox, PageLayout (Pages), a page Frame (Page), Table or BillboardGui. A screen element goes into the selected ScreenGui, Frame, PageLayout or Table, else the ScreenGui around the selection or the Workspace's first; with no ScreenGui at all, one is inserted first. Page inserts a Frame named Page into the selected PageLayout, creating a PageLayout (and a ScreenGui) when there isn't one. BillboardGui goes into the selected part.
- Like Roblox Studio, the selected element gets handles in the viewport: drag its body to move it and a square to resize it (see `ui_handles.rs`). The drag snaps when an edge or center comes within 8 points of an edge or center of another element, or of the screen, and a magenta guide line shows the alignment. A flex child is not snapped, because the layout places it. Where CSS sets `width` or `height`, resizing rewrites that CSS value (in its own units) instead of `Size`.
- While those handles are showing, a label under the element gives its size in screen pixels and how far it sits from the left, right, top and bottom of the screen. A child of a Frame, Table or other element also shows those distances to its parent, in magenta, and still uses the same handles. BillboardGui children keep those handles in the world view.
- A ScreenGui's `ScaleMode` is `Fixed` (the default: positions and sizes are as authored) or `Fit`. Fit lays that ScreenGui out at `DesignWidth` by `DesignHeight` (default 1920×1080), then scales it uniformly and centers it so the whole design stays proportional and inside the viewport on any screen size or aspect ratio. Dragging a handle still edits the design-pixel `Position`, `Size` or CSS length. Set the design size to the resolution you laid the UI out at.
- **Add a style** autocompletes property names and, for the supported ones, their values (flex, colors, lengths and so on). Tab takes the first match; clicking a suggestion fills it in. The same suggestions appear on a value that is already set, while that box is focused.
- Unlike Studio, the Properties panel also has a **Style (CSS)** section, like a browser's style inspector: the declarations that apply to the element, grouped by rule with a link to the rule's line, each value editable in place (colours with a swatch). Edits rewrite the stylesheet's text in its tab (unsaved until Ctrl+S) and restyle the viewport at once. **Add a style** puts `property: value` into the element's own `#Id` rule: an element without an `Id` gets one from its name, the rule is made at the end of the first stylesheet if it doesn't exist, and a project with no stylesheet gets `ui.css`.
- Stylesheets edited in another program (VS Code) restyle the viewport when saved there, unless the editor has unsaved changes to them, which are kept.

## Future (don't build yet, don't block)

A markup layer (HTML-like `.ui` files that produce Instances, with data binding). Keep Instance creation APIs general enough for a markup layer to use them.
