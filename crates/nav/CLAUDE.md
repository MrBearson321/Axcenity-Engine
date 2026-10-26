# `nav`

- The navigation mesh (`lib.rs`: walkable cells, links, regions, from `physics::solids`), paths across it (`path.rs`), and `Agents`, which steps NavigationAgents and steers them round new obstacles (`agents.rs`). Behavior: `docs/architecture/navigation.md`. Tests: `src/tests.rs` (mesh), `src/agent_tests.rs` (paths, agents).
- May depend on `core`, `assets`, `physics` and `character` (to leave out character bodies, and `body_parts`). `physics` and `character` must never depend on this crate. The player and editor Play each own an `Agents`.
- Building reads the world and never writes it. Agents only move their own body, and write `Destination` (while `Velocity` drives them) and `Status`. Keep `fingerprint` in step with what `build` reads, or the editor overlay won't rebuild after a change.
