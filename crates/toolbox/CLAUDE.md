# `toolbox`

- The Axcenity Toolbox: the editor's one way to find and fetch assets online. Today it holds the shared pieces the Get texture dialog (`editor/src/ambient_cg.rs`) uses: `http` (HTTPS with size limits, the `Http` trait, `Fixture` for tests), `guard` (untrusted names and zip extraction), `files` (`replace_dir`), `SourceError`. The design and the migration it's part of: `docs/research/asset-sources/` (open `provider-interface.md`).
- Editor only: `player`, `server`, `sim` and `testing` must never depend on it.
- No test reaches the network: use `http::Fixture`. Messages in `SourceError` must be safe to show users (no response bodies or headers).
- Every request sends `http::USER_AGENT`; never a browser's.
