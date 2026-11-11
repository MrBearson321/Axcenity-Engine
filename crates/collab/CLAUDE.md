# `collab`

- Team Create's sync core (phase TC1 of `docs/research/team-create.md`; ADR-0020): operations (`op.rs`), applying them with the conflict rules and their exact inverses (`apply.rs`), ID blocks (`ids.rs`), roles (`role.rs`), messages (`message.rs`), the host (`host.rs`) and the client (`client.rs`).
- Depends on `core` only: no network, files, UI or scripts. The transport, identity and editor wiring come in TC2 and TC3.
- Every world here is a replica-mode `World` (`World::make_replica`): synced Instances are made only at IDs from a writer's block (`create_at`).
- Tests: `tests.rs` (each conflict rule) and `convergence_tests.rs` (random edits over a simulated network with reconnects). Keep the conflict table in the design doc and the tests in step.
