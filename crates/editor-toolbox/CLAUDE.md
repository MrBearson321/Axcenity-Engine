# `editor-toolbox`

- The editor's Toolbox tab (G11's first panel crate): search, the details drawer, downloads, installs, packs and library views. Open `docs/editor/assets.md`.
- Builds only on `editor-kit` and the `toolbox` crate; it hands `ToolboxAction`s back, and the editor carries them out in `app/toolbox.rs` (it imports this crate as `toolbox_panel`).
- `agent.rs`: search and Get without the tab, for the editor's MCP tools `toolbox_search` and `toolbox_get` (tests: `agent::testing`).
- Never commit API keys; downloads follow the `toolbox` crate's rules for untrusted files.
