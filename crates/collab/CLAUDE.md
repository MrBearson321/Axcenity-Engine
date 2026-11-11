# `collab`

- Team Create's sync core (TC1 and TC2 of `docs/research/team-create.md`; ADR-0020): operations (`op.rs`), applying them with the conflict rules and their exact inverses (`apply.rs`), turning a World's edit journal into ops (`capture.rs`), ID blocks (`ids.rs`), roles (`role.rs`), messages (`message.rs`), the host (`host.rs`) and the client (`client.rs`).
- Depends on `core` only: no network, files, UI or scripts. The transport, identity and editor wiring come in TC2 and TC3.
- The client works on the editor's own World (passed in to every call), which records edits (`World::record_edits`); tools edit it directly and `Client::capture` sends each undo step. Synced Instances use IDs from the writer's blocks (`World::add_id_block`); every world here is in replica mode.
- Tests: `tests.rs` (each conflict rule), `capture_tests.rs` (edits made straight on a World) and `convergence_tests.rs` (random edits, as ops and straight on the World, over a simulated network with reconnects). Keep the conflict table in the design doc and the tests in step.
