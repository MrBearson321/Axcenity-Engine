# `editor-kit`

- The editor's shared UI base (roadmap G11): `theme` (design tokens, fonts, icons), `widgets`, `responsive` (layouts that fit a 260-point column), `color_palette`, `lua_syntax`, `wheel`, `screen`, and drawing tests' `snapshot`. Open `docs/editor/shell.md` ("Design system").
- Knows nothing about the editor's state: no `EditorApp`, tools or Instances beyond `core`'s class names. Editor-only icons (`tool_icon`, `insert_icon`) live in `editor/src/theme.rs`.
- The editor re-exports these modules at its crate root, so keep their public names stable. Test helpers (`theme::run`, `test_context`, `radius::ALL`, `snapshot`) are behind the `test-support` feature; turn it on only from `[dev-dependencies]`.
