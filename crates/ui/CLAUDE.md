# `ui`

- Turns ScreenGui, Frame, TextLabel, TextButton, ImageLabel, TextBox, PageLayout, Table and BillboardGui into screen-space draw data, applying parsed CSS stylesheets.
- Uses Taffy for absolute and flex layout; `UDim2` combines parent-relative scale and pixel offsets. `ScaleMode` Fit and Scale scale from the design size (`fit_transform`); `layout_artboard` lays one ScreenGui out for the UI Designer.
- `style.rs` parses selectors, specificity, variables and warnings. `lib.rs` lays out with taffy, `paint.rs` makes draw items, `css.rs` reads CSS values and `rich.rs` reads `RichText` tags into `UiText::spans`. Keep `render` free of Instances.
- `loading.rs`: `LoadingGui`, the project's ScreenGui loading screen; hosts pass `on_top` in `UiInput`.
- `interact.rs`: hosts keep one `UiInteraction` for hover, press, tooltips, the cursor and `UiEvent`s; `transition.rs` eases styles (`transition`) by resolving them through it. `layout_artboard_in_state` shows one element in a `UiState`.
