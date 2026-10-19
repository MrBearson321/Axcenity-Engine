# `ui`

- Turns `ScreenGui`, `Frame`, `TextLabel`, `TextButton` and `ImageLabel` Instances into screen-space draw data, applying parsed CSS stylesheets.
- Uses Taffy for absolute and flex layout; `UDim2` combines parent-relative scale and pixel offsets.
- `style.rs` owns selector parsing, specificity, variables and warnings; keep `render` Instance-free.
- Keep `render` free of Instances; this crate returns render draw-list types.
