# Game UI

Read before working in the `ui` crate. This covers in-game UI; the editor's own UI is `egui` and lives in `editor`.

## Model

- UI elements are **Instances**: `ScreenGui`, `Frame`, `TextLabel`, `TextButton`, `ImageLabel` (start set).
- Each has `Class` (space-separated class names) and `Id` properties, used by CSS selectors.
- Scripts and blocks change UI through normal properties and events (`Text`, `Visible`, `Activated`, and so on).

## Styling: a CSS subset (plain CSS, not SCSS)

- Parse with `cssparser` or `lightningcss`. `.css` files live in the project folder.
- Selectors: type (`TextButton`), `.class`, `#id`, descendant, `:hover`, `:active`.
- Properties: flexbox (`display:flex`, `flex-direction`, `justify-content`, `align-items`, `gap`, `flex-grow`, and so on), `width`/`height` (px and %), `padding`, `margin`, `background-color`, `color`, `border`, `border-radius`, `font-family`, `font-size`, `opacity`, CSS variables (`--name` / `var()`).
- Later: `transition`. Unsupported properties produce a warning in Output, never a crash.
- Specificity and cascade follow normal CSS rules for the supported subset.

## Layout and drawing

- Layout is computed by **`taffy`** (frame order step 5). Text is shaped with `cosmic-text` or `glyphon`.
- `ui` produces UI draw data, and `render` draws it.

## Future (don't build yet, don't block)

A markup layer (HTML-like `.ui` files that produce Instances, with data binding). Keep Instance creation APIs general enough for a markup layer to use them.
