# `ui`

- Turns `ScreenGui`, `Frame` and `TextLabel` Instances into screen-space draw data.
- Uses Taffy for proportional layout; `UDim2` combines parent-relative scale and pixel offsets.
- Keep `render` free of Instances; this crate returns render draw-list types.
