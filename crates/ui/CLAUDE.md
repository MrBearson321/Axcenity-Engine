# `ui`

- Turns ScreenGui, Frame, TextLabel, TextButton, ImageLabel, TextBox, PageLayout, Table, ScrollingFrame and BillboardGui into screen-space draw data, applying parsed CSS stylesheets.
- Uses Taffy for absolute and flex layout; `UDim2` combines parent-relative scale and pixel offsets. `ScaleMode` Fit and Scale scale from the design size (`fit_transform`); `layout_artboard` lays one ScreenGui out for the UI Designer.
- `style.rs` parses selectors, specificity, variables and warnings. `lib.rs` lays out with taffy, `paint.rs` makes draw items (`paint_box`: shadows, background, borders), `css.rs` reads CSS values and `rich.rs` reads `RichText` tags into `UiText::spans`. Keep `render` free of Instances.
- `loading.rs`: `LoadingGui`, the project's ScreenGui loading screen; hosts pass `on_top` in `UiInput`.
- Siblings are laid out in tree order and drawn (and hit) in `z_order`; `UiElement::clip` and each draw item's `clip` come from `overflow` and ScrollingFrames. `scroll.rs`: a ScrollingFrame's canvas (`UiScroll`) and bars.
- `turn.rs`: `Rotation` turns an element's subtree after it's laid out (`UiTurn` on each `UiElement`; hit tests turn the pointer back). `ScreenInsets` with `DeviceSafeInsets` lays a ScreenGui out inside `ArtboardView::safe_insets` (`gui_area`).
- `interact.rs`: hosts keep one `UiInteraction` for hover, press, tooltips, the cursor, scrolling (`UiScrolled`) and `UiEvent`s; `transition.rs` eases styles (`transition`) by resolving them through it. `layout_artboard_in_state` shows one element in a `UiState`.
