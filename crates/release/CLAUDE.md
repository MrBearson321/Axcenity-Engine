# `release`

- The engine version and channel (`version.rs`), this build's provenance (`build_info.rs` + `build.rs`), the release files in `releases/` (`records.rs`), their checks and the Version Explorer queries (`history.rs`), `check`/`prepare` and the release plan (`prepare.rs`), what a release publishes: notes, history file, manifest (`publish.rs`), and `install.json` (`install.rs`). `main.rs` is the `axcenity-release` CLI; `.github/workflows/release.yml` calls it.
- Design: `docs/research/release-system/`; how to write entries: `releases/README.md`; cutting a release: `docs/project/releases.md`.
- Keep it light: the player and server depend on it. No UI here; the editor and the MCP toolkit read `History` through its query methods, which return serializable values and typed `QueryError`s.
- `build.rs` reruns only on git HEAD/branch or `AXCENITY_*` env changes; don't make it watch the index or the clock.
