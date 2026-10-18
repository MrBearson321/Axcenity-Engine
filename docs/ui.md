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

## Future (don't build yet, don't block)

A markup layer (HTML-like `.ui` files that produce Instances, with data binding). Keep Instance creation APIs general enough for a markup layer to use them.
