# `net`

- Multiplayer (Milestone 14): `transport.rs` (renet channels over UDP or the in-memory loopback, one interface), `handshake.rs` (join request, version checks, refusals worded for both ends), `session.rs` (`Host`, `Join`), `discovery.rs` (LAN broadcast), `game.rs` (`Network`: `NetworkService` requests and `Players`). Docs: `docs/architecture/networking.md`.
- Hosts call `Network::receive` first in the frame and `send` before drawing; keep both in the frame order.
- Keep `Message::Hello` and `Refused` readable by later versions, so a newer engine can still say why it refuses. Raise `PROTOCOL_VERSION` when a message changes.
- Never block: sockets are non-blocking and every wait is counted in frame time.
