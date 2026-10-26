# `nav`

- The navigation mesh: walkable cells, their links and regions, built from `physics::solids`. Behavior: `docs/architecture/navigation.md`. Tests are in `src/tests.rs`.
- May depend on `core`, `assets`, `physics` and `character` (to leave out character bodies). `physics` and `character` must never depend on this crate; `player` doesn't use it yet.
- Building reads the world and never writes it. Keep `fingerprint` in step with what `build` reads, or the editor overlay won't rebuild after a change.
