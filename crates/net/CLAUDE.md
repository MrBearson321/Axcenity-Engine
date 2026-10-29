# `net`

- Multiplayer (Milestone 14): `transport.rs` (renet channels over UDP or the in-memory loopback, one interface), `handshake.rs` (join request, version checks, refusals worded for both ends), `session.rs` (`Host`, `Join`), `discovery.rs` (LAN broadcast), `wire.rs` (values and remote messages, Instances as network IDs), `replicate.rs` (Phase 64: the join snapshot and each frame's changes, found by comparing with what was sent), `game.rs` (`Network`: `NetworkService` requests, `Players`, sending the world, and remote messages; a joined device follows the server through `ScriptRuntime::become_client`). Docs: `docs/architecture/networking.md`.
- Hosts call `Network::receive` first in the frame and `send` before drawing; keep both in the frame order.
- Keep `Message::Hello` and `Refused` readable by later versions, so a newer engine can still say why it refuses. Raise `PROTOCOL_VERSION` when a message changes.
- Never block: sockets are non-blocking and every wait is counted in frame time.
- Nothing in ServerScriptService or ServerStorage (`World::is_server_only`) goes to a player: not in replication, not as a message's Instance. A player's message is theirs whatever it says (the connection decides its Player).
