# Post-processing

HDR, bloom, and tone mapping.

- The scene draws into an `Rgba16Float` texture. A bloom chain (prefilter above `BloomThreshold`, downsample, additive upsample) feeds a composite pass that adds bloom, applies `Exposure` and tone mapping into the real target (or, with FXAA on, into an LDR target that FXAA then smooths into the real one), then the UI draws on top. With MSAA on, the scene pass draws multisampled and resolves into the HDR texture before bloom. Anti-aliasing is a graphics setting: [shadows.md](shadows.md).
- Lighting properties `PostProcessingEnabled`, `Exposure`, `ToneMapping`, `BloomEnabled`, `BloomIntensity` and `BloomThreshold` configure it. Off means a plain clamp with no bloom, whatever the curve.
- `ToneMapping` (`Enum.ToneMapping`, `render::ToneMapping`) picks the curve, after exposure:
  - `Classic`: each channel under 0.6 unchanged, brighter ones roll off toward 1 (it shifts the hue of bright colors). The default, so a world saved without the property looks as it did.
  - `Neutral`: Khronos PBR Neutral. Takes off the 4% a non-metal reflects (less in the darkest colors), so a lit part shows about its picked sRGB color, then compresses highlights without shifting hue. The editor's templates (`project::Template`) write it into new worlds.
  - `Filmic`: Stephen Hill's ACES fit, after scaling light by 1/0.6 so a mid-grey stays about as bright. More contrast; very bright colors wash toward white.
  - `render/src/tone_mapping.rs` is a CPU copy of the curves (`ToneMapping::apply`) that must match `post.wgsl`; a GPU test checks they agree. Scripts set it with `game.Lighting.ToneMapping = Enum.ToneMapping.Filmic` (or the name), and blocks with "set tone mapping to …".
- `scene` draws Neon parts 2.5× brighter than their color when bloom is on, so they pass the threshold and glow.
