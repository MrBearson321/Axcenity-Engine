# Game UI

Read before working in the `ui` crate. This covers in-game UI; the editor's own UI is `egui` and lives in `editor`.

## Model

- UI elements are **Instances**: `ScreenGui`, `Frame`, `TextLabel`, `TextButton`, `ImageLabel` (start set).
- Each has `Class` (space-separated class names) and `Id` properties, used by CSS selectors.
- Scripts and blocks change UI through normal properties and events (`Text`, `Visible`, `Activated`, and so on).

## Styling: a CSS subset (plain CSS, not SCSS)

- Plain `.css` files anywhere in the project folder are loaded in path order.
- Selectors: type (`TextLabel`), `.class`, `#id`, comma lists, and descendant selectors. Specificity and later-source order follow CSS cascade rules.
- Properties: flex layout (`display`, `flex-direction`, `justify-content`, `align-items`, `gap`, `flex-grow`), `width`/`height` (px and %), `padding`, `margin`, `background-color`, `color`, `border`, `border-radius`, `font-family`, `font-size`, `opacity`, CSS variables (`--name` / `var()` with fallbacks).
- `Visible = false` removes an element from flex layout and hit testing. `:hover` follows the topmost visible element under the pointer; `:active` applies to the visible button being held while the pointer remains over it. Unsupported properties produce a warning in Output, never a crash.
- Specificity and cascade follow normal CSS rules for the supported subset.

## Layout and drawing

- Layout is computed by **`taffy`** (frame order step 5). Text is shaped with `cosmic-text` or `glyphon`.
- `ui` resolves styles and produces UI draw data, and `render` draws it as an overlay. The editor viewport previews UI while editing as well as during Play; edit-mode scripts do not run.
- `TextButton.Activated` fires on release after a press that began and ended over the same visible button. Only LocalScripts receive it. `ImageLabel.Image` is a project-relative asset path.

## Editing UI in the editor

- The toolbar's UI group inserts a ScreenGui, Frame, TextLabel (Text), TextButton (Button) or ImageLabel (Image). An element goes into the selected ScreenGui or Frame, else the ScreenGui around the selection or the Workspace's first; with no ScreenGui at all, one is inserted first.
- Like Roblox Studio, the selected element gets handles in the viewport: drag its body to move it and a square to resize it (see `ui_handles.rs`). Where CSS sets `width` or `height`, resizing rewrites that CSS value (in its own units) instead of `Size`.
- Unlike Studio, the Properties panel also has a **Style (CSS)** section, like a browser's style inspector: the declarations that apply to the element, grouped by rule with a link to the rule's line, each value editable in place (colours with a swatch). Edits rewrite the stylesheet's text in its tab (unsaved until Ctrl+S) and restyle the viewport at once. **Add a style** puts `property: value` into the element's own `#Id` rule: an element without an `Id` gets one from its name, the rule is made at the end of the first stylesheet if it doesn't exist, and a project with no stylesheet gets `ui.css`.
- Stylesheets edited in another program (VS Code) restyle the viewport when saved there, unless the editor has unsaved changes to them, which are kept.

## Future (don't build yet, don't block)

A markup layer (HTML-like `.ui` files that produce Instances, with data binding). Keep Instance creation APIs general enough for a markup layer to use them.
