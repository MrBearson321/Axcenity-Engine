# Post-processing

HDR, bloom, and tone mapping.

- The scene draws into an `Rgba16Float` texture. A bloom chain (prefilter above `BloomThreshold`, downsample, additive upsample) feeds a composite pass that adds bloom, applies `Exposure` and tone mapping (colors under 0.6 are unchanged, brighter ones roll off toward white) into the real target, then the UI draws on top.
- Lighting properties `PostProcessingEnabled`, `Exposure`, `BloomEnabled`, `BloomIntensity` and `BloomThreshold` configure it. Off means a plain clamp with no bloom.
- `scene` draws Neon parts 2.5× brighter than their color when bloom is on, so they pass the threshold and glow.
