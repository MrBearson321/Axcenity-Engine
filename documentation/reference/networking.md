---
description: Multiplayer as it is today, server-authoritative with replication, hosting, joining and LAN discovery, testing with several clients in the editor, network ownership, and world streaming.
icon: globe
status: partial
samples: [hello-remote, hello-streaming]
---

# Multiplayer and networking

Multiplayer works on a local network and between friends who connect directly. It isn't ready for public internet servers: connections aren't encrypted or authenticated yet, and there's no matchmaking or relay.

## How it works

The **server** owns the game: it runs Scripts and physics and sends each player what changed. Each player's device keeps a copy of the world and runs only LocalScripts. Anything in ServerScriptService or ServerStorage never leaves the server.

- When a player joins, the server checks the game, its version, the password and `Players.MaxPlayers` (8 to start), then sends a snapshot of the world.
- Each frame it sends property and tree changes reliably, and moving parts' positions on a faster channel that can drop packets, most important first within a budget.
- Players send their own character's movement and RemoteEvent messages. The server checks how far a character may move each update.

## Hosting and joining

| From | Host | Join |
|---|---|---|
| The player | `axcenity-player --host <project>` | `axcenity-player --join 192.168.1.20:47800 <project>` |
| A dedicated server | `axcenity-server <project>` (no window, graphics or audio) | — |
| A script | `NetworkService:Host()` | `NetworkService:Join()`, or find games with `NetworkService:FindLanGames()` |

Games use UDP port **47800**, and **47801** to find games on the local network. `--password` protects a game. `NetworkService.ConnectionFailed` says why a join failed.

## Testing in the editor

Set **Clients** (1 to 8) in the Home tab's Game group before Play to run several players in the editor, joined over a simulated network; **Network** adds latency, jitter and packet loss. The view menu switches between the server and each client, and the Profiler's Network section shows traffic. The editor's Play can't host or join a real network game.

## Scripting for multiplayer

- Talk between the sides with **RemoteEvents** and **RemoteFunctions** (see [Scripting reference](scripting.md)). Check everything a player sends: their device is theirs to change.
- `Players.PlayerAdded` and `Players.PlayerRemoving` fire as people come and go; `Players.LocalPlayer` is this device's player in a LocalScript.
- `Part:SetNetworkOwner()` lets one player's device simulate an assembly they control, such as a car; `Part:SetNetworkOwnershipAuto()` gives it back to the nearest player.

## World streaming

**Partly built, off to start.** With `Workspace.StreamingEnabled` on, each player has only the part of the world near them: the server sends what's within `Workspace.StreamingTargetRadius`, and their device frees what's far away, including its GPU copies, and lets far physics sleep. Scripts check `Instance.StreamingState` and wait for `Instance.StreamedIn`, or ask for an area with `Player:RequestStreamAroundAsync()`. A Model's `Model.ModelStreamingMode` keeps it whole (Atomic) or always loaded (Persistent).

> **Known limits.** No encryption or connect tokens: LAN and trusted direct play only. No relay, matchmaking, lag compensation or replays. Tested with 8 players; larger LAN games haven't been measured. A RemoteEvent message that arrives before a handler is connected is dropped.

More: [Networking](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Engine-Systems/Networking.md) and [World streaming](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Engine-Systems/World-Streaming.md) in the engineering wiki.
