# Networking

Hosting, joining and finding games (Milestone 14). Phase 63 is the transport, hosting and joining. Phase 64 (so far, for hosted games): a device that joins follows the server's world as a replica and runs only its LocalScripts there, RemoteEvents and RemoteFunctions cross between devices, and Players carry `UserId` and `DisplayName`. Phase 65: parts move by quantized motion within each player's budget and are smoothed on their device, and the level state follows the server. Single-player and the editor's Play still run both sides on one World; they move to a server and client over loopback (with Play's Clients 1–8) together with client-owned characters.

## Crates

- `net` (`axcenity-net`): the transport, the join handshake, LAN discovery, the wire format (`wire.rs`), replication (`replicate.rs`) and `Network`, which the hosts run. It depends on `core` and `script`.
- `server` (`axcenity-server`): the dedicated server binary.
- `script`'s `NetworkService` (`network.rs`) never touches the network: it queues `NetworkRequest`s, and the host answers them.

## Transport

`net::transport` uses `renet` 2 (reliable-ordered and unreliable channels) over UDP through `renet_netcode` (netcode's connection handshake, 15-second timeout and disconnect reasons). The in-memory loopback has the same `Server` and `Client` interface; `Network::host_in_memory` and `join_through` use it.

- Channel 0 is reliable-ordered, channel 1 unreliable (`Channel`). The unreliable channel carries motion (below).
- The loopback can emulate a worse network (`Conditions`: `latency` added to each packet each way, give or take `jitter`, and the share `loss` dropped; `Server::emulate`, `Network::emulate`). `Network::stats` gives each connection's round trip, loss and bytes a second (renet's numbers), and the bytes sent over a loopback.
- Netcode runs unsecured: there's no account service to hand out connect tokens, so packets aren't encrypted. This is enough for direct-address and LAN play, which is all there is (no relay or matchmaking).
- Netcode's protocol id is the same for every game and version. Games and versions are told apart by the handshake, which can say why it refuses; netcode would drop the packets silently.
- A server listens on IPv4 (`0.0.0.0`), and joining prefers an address's IPv4 form.

## Handshake

The joining player's first message, on the reliable channel as JSON, is `Hello`: `PROTOCOL_VERSION` (3 since Phase 65), the game's name, its version (`version` in `axcenity.json`, `"1.0"` when it isn't set), the player's name, the password and their `user_id` (missing from older engines, read as 0). The host checks them in this order and refuses with a reason worded for each side (`handshake::admit`):

| Check | The player's `ConnectionFailed` reason | The host's log |
|---|---|---|
| Network protocol | "The server uses network protocol 1 and you use 2: play the game with the same Axcenity version." | "refused a player using network protocol 2; …" |
| Game | "The server runs "Castle", not "Keep"." | "refused a player of another game, …" |
| Version | "Version mismatch: the server runs version 1.0 of "Castle" and you have version 2.0." | "refused a player with version 2.0 of "Castle": this server runs version 1.0" |
| Password | "This game needs a password." or "Wrong password." | "refused a player with the wrong password" |
| `Players.MaxPlayers` (8; the host's own player counts) | "The game is full (8 of 8 players)." | "refused a player: the game is full (8 of 8)" |

A refused connection stays open for a second, so the reason arrives, then closes. A connection that says nothing for 10 seconds is closed. A player who hears nothing back for 10 seconds gives up ("The server didn't answer the join request."). Netcode's own failures become reasons too: "No game answered at …", "The connection timed out…", "The server is full." and "The server closed the connection."

Accepted, the player gets `Welcome`: their id (a listen server's own player is 1), name, made unique (a second `Player` is `Player2`), display name (the name they asked for) and user id, and everyone already in. `Joined` and `Left` keep every roster the same. `Hello` and `Refused` must stay readable by later engines; raise `PROTOCOL_VERSION` when a message changes.

## Players

On the host, `Network::receive` turns the roster into Players: `ScriptRuntime::add_remote_player` makes a Player in `Players` with its `DisplayName` and `UserId` (on a team; `PlayerAdded` fires; it gets no character yet, that's network ownership in Phase 66), and `remove_player` fires `Players.PlayerRemoving`, then destroys the Player and its character. On a player's device, Players come with the server's world: one that arrives fires `PlayerAdded` in its LocalScripts (`add_replicated_player`), and one the server removes fires `PlayerRemoving` there before it goes. A failure, a refusal or `Disconnect` removes every other player, from a replica too.

- `Player.UserId` is the install's identity: a random whole number from 1 to 2⁵³−1, kept in `user-id.txt` in the saves folder (`axcenity_save::user_id`, beside every game's save folder), made the first time. There are no accounts. A dedicated server has no player, so no UserId. The editor's tests use a fixed one so they never write it.
- `Player.DisplayName` is the name they joined with; `Name` is that, numbered if taken. Both are read-only to scripts.

## Worlds and replication

Each device has its own World. The server's is the game; a player's device that joined keeps a **replica**: `World::empty_replica` (the services only, at the same IDs, with the device's own string table, language, UI settings, collision groups and volume), filled with `World::create_at`. The replica's arena holds the server's Instances at their own IDs and puts the device's own (what LocalScripts make) in a range the server never uses (`InstanceId::is_local`), so an `InstanceId` is the network ID: `to_bits` and `from_bits` name the same Instance everywhere.

- **What's sent** (`replicate.rs`): `game`'s properties and every service but the server-only ones, ServerScriptService and ServerStorage (`World::server_only_services`), with everything inside. A player's replica has those two services, empty.
- **Joining**: the host sends `Snapshot` (which Player is theirs, then each service's properties and every Instance, parents first). The device stops every script (`ScriptRuntime::become_client`), swaps its world for the replica, copies its own copy's Terrain landscapes by ID, sets `LocalPlayer`, and starts the game's ModuleScripts and LocalScripts again (`Network::set_client_scripts`; the same game version loads them at the same IDs). From then on only LocalScripts run there; Scripts given to `start_script` are skipped.
- **Each frame**, in network send, `Replicator::frame` compares the world with what it last sent and sends `Update`: made Instances, changed names, parents, properties, attributes and tags, and Instances destroyed, taken out of the game or moved into a server-only service. It's reliable JSON, with the server's clock (`time`). Instance properties are applied last, so they can name an Instance later in the update. Comparing, rather than reading `core`'s change records, means nothing is missed though the scripts take those records first.
- **Motion** (`motion.rs`): a BasePart's `Position`, `Orientation`, `AssemblyLinearVelocity` and `AssemblyAngularVelocity` aren't in updates once the part is on a device. Each frame each player's `MotionSender` adds to every moved part's priority (how far it is from what they have, times nearness to their character: 1 / (1 + distance / 64)) and sends the highest first while their budget lasts (`MOTION_BUDGET`, 24 000 bytes a second, saved up for at most a quarter second). A part is 32 bytes: its network ID, its position to 1/128 stud, each angle to 360/65536°, and its velocity to 1/32 stud a second, in packets under 1000 bytes on the unreliable channel. A part still for `SETTLE_TIME` (0.2 s) is sent once more, exactly, in a reliable update, so a lost packet never leaves it elsewhere. 200 moving crates stay under 32 000 bytes a second per player (`two_hundred_moving_crates_stay_under_the_bandwidth_budget_per_client`).
- **Smoothing**: a player's device shows parts `DELAY` (0.1 s) behind the newest motion it has, blending the two states around that moment (angles the short way round), with its clock drifting toward the server's. A part that starts moving after standing still starts from where it stood. A reliable update that places a part (made, or settled) makes older motion stale.
- **Levels**: an update carries `LevelService`'s level and the added ones (with their Models' IDs) when they change, and a snapshot always does, so a player who joins mid-game has the same added levels. When the server loads another level, `LevelLoaded` fires on the players' devices and their loading screen shows (`ScriptRuntime::follow_levels`).
- **A device's own changes** to its replica stay on it, until the server changes the same thing.
- **A device following a server** (`axcenity-player --join`, or `NetworkService:Join`) doesn't step physics, characters, Humanoids, players, agents, prompts, tools, health or touches: the server does. Its own character's movement is Phase 66. After leaving, it keeps the last world without the other players.

## Remote calls

RemoteEvents and RemoteFunctions use the same `Message`s as on one device. The runtime keeps a message for another device (to a Player who joined from elsewhere, or from a replica to the server) in `take_remote_messages`; `Network::send` sends it as `Remote` (`wire::WireMessage`: Instances as network IDs, Lua strings as bytes). The receiver hands it to `receive_remote_messages`, and it fires in the next frame's script step.

- `FireClient`, `FireAllClients` (every Player, wherever they play) and `InvokeClient` reach players on other devices; `FireServer` and `InvokeServer` reach the server.
- The host takes a message from player *n* as theirs: its Player is the connection's, never what the message says. It must name a RemoteEvent or RemoteFunction outside the server-only services, or it's dropped. An Instance argument a player can't see (server-only, or the device's own) arrives as nil, both ways.

## Hosting and joining

- `axcenity-player --host [port] <game>` runs a listen server (port 47800 if not given). `--join address:port` joins one. `--password <text>` sets a host's password, or gives it when joining. A port that's taken, or an address that can't be looked up, stops the player with the reason.
- `axcenity-server <game> [--port <port>] [--password <text>]` loads a project folder or `.axgame` and runs its Scripts (and the ModuleScripts they require, never LocalScripts) with physics at 60 Hz, with no window, audio or GPU and no player of its own (`ScriptRuntime::set_dedicated`). Its frame is network receive, scripts, `Stepped`, physics, touches, breaks, health, `Heartbeat`, network send. Characters, agents and animation belong to players' devices. On Linux it needs no display; `script` still links `audio`'s library, which needs ALSA's `libasound` installed (not a sound device). `tests/headless.rs` runs the real binary with `DISPLAY` and `WAYLAND_DISPLAY` unset.
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

Network receive comes first in the frame: `NetworkService` requests from last frame, then what arrived (roster, the world, remote messages). Network send (`Network::send(world, scripts)`) comes after the frame's scripts, physics and UI layout, just before rendering: the world's changes, then remote messages, so a message never names an Instance its receiver doesn't have yet. See [frame-loop.md](frame-loop.md).
