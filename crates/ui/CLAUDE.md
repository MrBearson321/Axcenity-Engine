# `ui`

- Turns ScreenGui, Frame, TextLabel, TextButton, ImageLabel, TextBox, PageLayout, Table and BillboardGui into screen-space draw data, applying parsed CSS stylesheets.
- Uses Taffy for absolute and flex layout; `UDim2` combines parent-relative scale and pixel offsets. `ScaleMode` Fit and Scale scale from the design size (`fit_transform`); `layout_artboard` lays one ScreenGui out for the UI Designer.
- `style.rs` parses selectors, specificity, variables and warnings. `lib.rs` lays out with taffy, `paint.rs` makes draw items and `css.rs` reads CSS values. Keep `render` free of Instances.
