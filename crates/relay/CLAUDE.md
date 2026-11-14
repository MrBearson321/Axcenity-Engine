# `relay`

- `axcenity-relay` (multiplayer readiness MP5, ADR-0026 D2): a host registers over TCP and gets a join code; players find it by code, try a direct path (hole punching) and otherwise send their UDP through the relay. Docs: `docs/architecture/networking.md` (Reaching a host).
- `protocol.rs` is shared with `net` (the host's link and the player's join): UDP packets start `AXR`, control messages are JSON lines. Change both ends together.
- The relay never sees the game: netcode encrypts packets and the token exchange is TLS end to end; it only splices bytes. Keep it std-only threads, no async runtime, and no state on disk.
