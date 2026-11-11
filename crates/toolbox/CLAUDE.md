# `toolbox`

- The Axcenity Toolbox: the editor's one way to find and fetch assets online. Sources implement `AssetSource` (`source.rs`) and return `AssetRecord`s (`record.rs`, the shared `asset-source` record): `ambientcg.rs` (v3 API), `polyhaven.rs` (catalogue searched locally, `/info` + `/files`), `openverse.rs` (sounds, per-asset licences). Under them: `http` (`Http` trait, `Client`, `Fixture`), `guard`, `files`, `license`, `SourceError`. Get texture (`editor/src/ambient_cg.rs`) uses `http`, `guard` and `files` so far. Design and plan: `docs/research/asset-sources/` (open `provider-interface.md`).
- Editor only: `player`, `server`, `sim` and `testing` must never depend on it (`scripts/check-platform.*` checks the player).
- Unit tests never reach the network: use `http::Fixture`. `tests/live.rs` reaches the real sources and is ignored; run it by hand after changing a source (`--run-ignored only`).
- `SourceError` messages must be safe to show (no bodies or headers). Every request sends `http::USER_AGENT`, never a browser's. Never offer `.blend` files; check each file's URL with `SourceInfo::allows_download_from`.
