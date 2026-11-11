---
description: "Hosting and joining games, from game:GetService(\"NetworkService\"), so a menu can do them."
---

# NetworkService

Hosting and joining games, from `game:GetService("NetworkService")`, so a menu can do them. Players who join appear in `Players` on every device (the worlds aren't shared yet). Both sides have it; in the editor's Play, Host and Join fail and FindLanGames finds nothing.

### Properties

#### ConnectionFailed

```luau
NetworkService.ConnectionFailed: Signal
```

Fires with why when hosting or joining fails, a join is refused (such as a version mismatch, a wrong password or a full game) or the connection is lost: `function(reason) ... end`.

### Functions

#### Host

```luau
NetworkService:Host(port: number?, password: string?): ()
```

Hosts the game on this device on UDP `port` (47800 if left out), for at most `Players.MaxPlayers` players; with a `password`, only players who give it can join. ConnectionFailed fires if the port is taken.

#### Join

```luau
NetworkService:Join(address: string, password: string?): ()
```

Joins the game hosted at `address`, such as `NetworkService:Join("192.168.1.20:47800")`, with its `password` if it has one. The player on this device may be renamed (a second `Player` is `Player2`). ConnectionFailed fires with why if it fails or is refused.

#### FindLanGames

```luau
NetworkService:FindLanGames(): { { Name: string, Version: string, Address: string, Players: number, MaxPlayers: number, HasPassword: boolean } }
```

Asks the local network for hosts of this game, waits about a second (it pauses the script), and returns what answered: `{ Name, Version, Address, Players, MaxPlayers, HasPassword }` each, `Address` ready for Join.

#### Disconnect

```luau
NetworkService:Disconnect(): ()
```

Leaves the game joined, or stops hosting; the other players leave `Players`.

