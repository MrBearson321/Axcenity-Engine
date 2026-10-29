# Networking

Hosting, joining and finding games (Milestone 14). Phase 63 is the transport, hosting and joining: players who join appear in every device's `Players`, but each device still runs its own world. Shared worlds and remote calls are Phase 64, replication Phase 65.

## Crates

- `net` (`axcenity-net`): the transport, the join handshake, LAN discovery and `Network`, which the hosts run. It depends on `core` and `script`.
- `server` (`axcenity-server`): the dedicated server binary.
- `script`'s `NetworkService` (`network.rs`) never touches the network: it queues `NetworkRequest`s, and the host answers them.

## Transport

`net::transport` uses `renet` 2 (reliable-ordered and unreliable channels) over UDP through `renet_netcode` (netcode's connection handshake, 15-second timeout and disconnect reasons). The in-memory loopback has the same `Server` and `Client` interface; `Network::host_in_memory` and `join_through` use it.

- Channel 0 is reliable-ordered, channel 1 unreliable (`Channel`).
- Netcode runs unsecured: there's no account service to hand out connect tokens, so packets aren't encrypted. This is enough for direct-address and LAN play, which is all there is (no relay or matchmaking).
- Netcode's protocol id is the same for every game and version. Games and versions are told apart by the handshake, which can say why it refuses; netcode would drop the packets silently.
- A server listens on IPv4 (`0.0.0.0`), and joining prefers an address's IPv4 form.

## Handshake

The joining player's first message, on the reliable channel as JSON, is `Hello`: `PROTOCOL_VERSION`, the game's name, its version (`version` in `axcenity.json`, `"1.0"` when it isn't set), the player's name and the password. The host checks them in this order and refuses with a reason worded for each side (`handshake::admit`):

| Check | The player's `ConnectionFailed` reason | The host's log |
|---|---|---|
| Network protocol | "The server uses network protocol 1 and you use 2: play the game with the same Axcenity version." | "refused a player using network protocol 2; …" |
| Game | "The server runs "Castle", not "Keep"." | "refused a player of another game, …" |
| Version | "Version mismatch: the server runs version 1.0 of "Castle" and you have version 2.0." | "refused a player with version 2.0 of "Castle": this server runs version 1.0" |
| Password | "This game needs a password." or "Wrong password." | "refused a player with the wrong password" |
| `Players.MaxPlayers` (8; the host's own player counts) | "The game is full (8 of 8 players)." | "refused a player: the game is full (8 of 8)" |

A refused connection stays open for a second, so the reason arrives, then closes. A connection that says nothing for 10 seconds is closed. A player who hears nothing back for 10 seconds gives up ("The server didn't answer the join request."). Netcode's own failures become reasons too: "No game answered at …", "The connection timed out…", "The server is full." and "The server closed the connection."

Accepted, the player gets `Welcome`: their id (a listen server's own player is 1) and name, made unique (a second `Player` is `Player2`; the joiner's own Player is renamed), and everyone already in. `Joined` and `Left` keep every roster the same. `Hello` and `Refused` must stay readable by later engines; raise `PROTOCOL_VERSION` when a message changes.

## Players

`Network::receive` turns the roster into Players: `ScriptRuntime::add_remote_player` makes a Player in `Players` (on a team; `PlayerAdded` fires, and it gets no character yet), and `remove_player` fires `Players.PlayerRemoving` on both sides, then destroys the Player and its character. A failure, a refusal or `Disconnect` removes every other player. The local player keeps the name the host gave it.

## Hosting and joining

- `axcenity-player --host [port] <game>` runs a listen server (port 47800 if not given). `--join address:port` joins one. `--password <text>` sets a host's password, or gives it when joining. A port that's taken, or an address that can't be looked up, stops the player with the reason.
- `axcenity-server <game> [--port <port>] [--password <text>]` loads a project folder or `.axgame` and runs its Scripts (and the ModuleScripts they require, never LocalScripts) with physics at 60 Hz, with no window, audio or GPU and no player of its own. Its frame is network receive, scripts, `Stepped`, physics, touches, breaks, health, `Heartbeat`, network send. Characters, agents and animation belong to players' devices. On Linux it needs no display; `script` still links `audio`'s library, which needs ALSA's `libasound` installed (not a sound device). `tests/headless.rs` runs the real binary with `DISPLAY` and `WAYLAND_DISPLAY` unset.
- The player prints who joined and left, and every refusal and failure, to stderr (`Network::take_log`).
- The editor's Play doesn't host or join: `Host` and `Join` fire `ConnectionFailed` with a reason, and `FindLanGames` returns an empty list.

## LAN discovery

A host answers on UDP port 47801 (`DISCOVERY_PORT`, `discovery.rs`). `FindLanGames` broadcasts a question to `255.255.255.255:47801` and to `127.0.0.1:47801` (a broadcast may not come back to the computer that sent it), asks again every 0.3 s, and after one second lists the hosts of the same game, with their version, address, players, `MaxPlayers` and whether they need a password. A host found at two addresses is listed once, by its network address. Only the first host on a computer gets the discovery port; another still takes joins by address and logs a warning.

## Lua and blocks

`game:GetService("NetworkService")` (also `game.NetworkService`, on both sides):

- `Host(port?, password?)`, `Join(address, password?)`, `Disconnect()`.
- `FindLanGames()` pauses the script and returns `{ Name, Version, Address, Players, MaxPlayers, HasPassword }` tables.
- `ConnectionFailed(reason)`.

Blocks (in **More**): "host game on port *n*", "join game at *address*", "leave game", and the event "when connection fails" (`NetworkService:Host(n)`, `NetworkService:Join(address)`, `NetworkService:Disconnect()`, `NetworkService.ConnectionFailed:Connect(function(reason) … end)`).

## Frame order

Network receive comes first in the frame: `NetworkService` requests from last frame, then what arrived. Network send comes after the frame's scripts, physics and UI layout, just before rendering. See [frame-loop.md](frame-loop.md).
