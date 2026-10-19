# `ui`

- Turns ScreenGui, Frame, TextLabel, TextButton, ImageLabel, TextBox, PageLayout, Table and BillboardGui into screen-space draw data, applying parsed CSS stylesheets.
- Uses Taffy for absolute and flex layout; `UDim2` combines parent-relative scale and pixel offsets.
- `style.rs` parses selectors, specificity, variables and warnings. `lib.rs` lays out with taffy and paints draw items. Keep `render` free of Instances.
