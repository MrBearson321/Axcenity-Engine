# Multiplayer readiness: the plan for public hosting

Paul's priority 1 of 2026-11-14 is to make multiplayer ready for public hosting: scale tests with 16–32 real clients, secure connections, NAT traversal and a relay, general-purpose prediction and rollback, server-authoritative validation, and repeatable operational tests. The development policy deferred internet hosting and renet's secure mode (roadmap M11) "until the multiplayer systems are mature"; Paul's request lifts that deferral for the engine's own code. Running servers for other people (a public relay, matchmaking) is still infrastructure and stays Paul's call ([§4](#4-what-stays-pauls-call)).

- **Status:** MP0 (this plan) written. MP1's bots and harness are built ([§6](#6-results)); its two-machine run is G13, on Paul's PC and Mac. MP2's reconnecting and operational tests are built ([§6](#mp2-crashes-churn-and-floods-cloud-container-2026-11-14)). MP3 is built in two parts: MP3a (budgets, argument limits, flags, `Players.PlayerFlagged`, the kick threshold) and MP3b (wall and flight checks, owned-assembly speed, strikes, `--cheat` bots). MP4's secure connections are built ([§6](#mp4-secure-connections-cloud-container-2026-11-14)); its PC–Mac join over Tailscale waits for Paul's machines. MP5–MP7 not started.
- **Date:** 2026-11-14
- **Engine:** `main` at `f6d5427`
- **Relies on:** [ADR-0010](../../wiki/Architecture/Architecture-Decisions/ADR-0010-Listen-Server-Replicas-And-Network-IDs.md) (listen servers, replicas, network IDs), [ADR-0026](../../wiki/Architecture/Architecture-Decisions/ADR-0026-Secure-Game-Connections-And-Relay.md) (this plan's connection decisions), the [development policy](../development-policy.md) (no cross-machine determinism, no infrastructure before it's needed), [networking.md](../architecture/networking.md).

## 1. Summary

- The netcode is server-authoritative already: the server owns the world, replicates by comparison, budgets motion per player and checks each player's character ([networking.md](../architecture/networking.md)). Nothing is rewritten. The work adds what a server on the open internet needs around it.
- **Order:** measure first (bots and the scale run), then make the server hard to break (operational tests, validation), then make it reachable safely (secure connections, NAT and relay), then prediction and rollback, which is the largest and most open piece.
- **Paul's machines** are needed for the 16–32 client run (PC and Mac, LAN and Tailscale) and the cross-network NAT checks. Everything else is built and tested in the cloud container with real UDP sockets on one machine.

## 2. Where things stand

| Item | What exists (main `f6d5427`) | Gap |
|---|---|---|
| Scale testing | `axcenity-server --bench <frames> --bench-clients <n>`: n loopback players in the server's process (M7: S10k with 8 players, 1.02 MB snapshot, 5.06 ms network send). Editor Play with up to 8 clients. | No client that runs as its own process over real UDP, so no test with players on other machines. No per-player numbers over time (round trip, corrections, bytes) from a long run. `MaxPlayers` defaults to 8. |
| Secure connections | `renet_netcode` runs `ServerAuthentication::Unsecure` (`transport.rs`): packets are neither encrypted nor signed, and anyone who knows the address can connect. A password is checked in `Hello`, which travels in the clear. | Encryption, connect tokens, a server identity the player can check. |
| NAT traversal and relay | Direct address, LAN discovery (UDP 47801). Collaborate uses Tailscale (C3), games don't. | A host behind a home router can't be reached without a forwarded port. No port mapping, hole punching, relay or join code. |
| Prediction and rollback | A player's device moves its own character and owned assemblies with no delay; the server checks and corrects them (`ownership.rs`). Others are shown 0.1 s behind, smoothed (`motion.rs`). Server raycasts can be lag compensated, and replays record the replication stream (Phase 81). | No input-driven simulation, so no rewind and resimulate: an owned assembly is trusted within 60 studs, a correction snaps, and a game can't predict anything else (a projectile, a door, a vehicle the server drives). |
| Server-authoritative validation | `CharacterCheck` (speed, jump height, 30-stud teleports), owned assemblies within 60 studs, remote messages from a player are theirs and must name a RemoteEvent outside server-only services, chat trimmed to 200 characters, console commands only for `AllowedUserIds`. | No rate or size budgets per connection (a client can fill the 5 MiB reliable channel with RemoteEvent calls), no check that a character moved through walls or kept flying, owned assemblies' speed unchecked, no strike count or kick for repeated failures, nothing a game's Scripts can read about failed checks. |
| Operational testing | `crates/server/tests/headless.rs` starts the real binary; LAN discovery has a two-computer hand test. | No tests for a server crash and restart, players reconnecting, many joins and leaves (leaks), floods and exhaustion, or recovery. A client never reconnects by itself. |

## 3. Decisions

Recorded in [ADR-0026](../../wiki/Architecture/Architecture-Decisions/ADR-0026-Secure-Game-Connections-And-Relay.md) where they touch connections. Each can be changed by Paul; the phases below follow them until he does.

### D1. Secure connections are netcode connect tokens that the game server issues itself

renet's secure mode needs a connect token per player, made with a private key the server knows, and delivered to the player privately (the token carries the keys that encrypt the connection). Netcode leaves "who hands out tokens" to the game. Options:

| Option | For | Against |
|---|---|---|
| **A. The server issues tokens itself over TLS, pinned by its certificate's fingerprint** | No account service or website. The same model as Collaborate (C3/C10: self-signed certificate, fingerprint in the link). Works for a listen server and `axcenity-server` alike. | A typed address with no fingerprint has to trust the first certificate it sees (as SSH does). |
| B. A separate token service (web back end) | The standard netcode setup; lets accounts decide who may join. | Infrastructure the policy rules out, and an account system (R19) that doesn't exist. |
| C. Encrypt in Axcenity's own layer over unsecured netcode | No token exchange. | Hand-rolled cryptography; netcode's secure mode already does this properly. |

**Chosen: A.** A player first opens a TLS connection (rustls, already used by Collaborate) to the server's **token port**, TCP on the same number as the game's UDP port (47800), sends who they are, and gets a connect token valid for 30 seconds. Then netcode connects with it, encrypted. The `Hello` handshake runs as before inside the encrypted connection, so refusals keep their wording. The fingerprint comes from a join link or server list entry (`axcenity://join/<host:port>#<fingerprint>`); a typed address remembers the first fingerprint per address in the player's data folder and refuses a changed one with why. A password is checked at the token port, so a wrong password never gets a token. LAN play, the editor's Play and the loopback stay unsecured (nothing crosses a network, or the user chose LAN): `--host --lan` and LAN discovery keep today's behaviour. B stays possible later: the token format is netcode's, so an account service could issue the same tokens with the server's key.

### D2. Reaching a host: direct, then port mapping, then hole punching, then a relay

| Step | What it does | Needs |
|---|---|---|
| Direct | As today: an address and port the player can reach. | A public address or a forwarded port. |
| Port mapping | The host asks the router to forward its game and token ports (UPnP IGD, then NAT-PMP/PCP), and drops the mapping when it stops. | A router that allows it (Paul's doesn't have UPnP, 2026-11-12). |
| Hole punching | Host and player both reach a rendezvous service, learn each other's public addresses and send UDP to each other at once. | A rendezvous service both can reach; fails on symmetric NAT. |
| Relay | The relay forwards UDP between player and host; netcode's encryption (D1) means it can't read or alter the game. | A relay both can reach. |

**Chosen:** all four, tried in that order and reported in the join's log. The rendezvous and relay are one self-hostable binary, `axcenity-relay`, in the repository: a host registers with it and gets a short **join code**; a player joins by code. Axcenity runs no public relay (policy: no infrastructure); a game, or Paul, runs one wherever they like. Tailscale stays a supported path: an address on the tailnet is just a direct address.

### D3. Prediction and rollback without cross-machine determinism

The policy rules out deterministic lockstep (no `enhanced-determinism`, no fixed-point). Rollback here is **client-side prediction with server reconciliation**: the server stays the authority and simulates from the inputs players send; a player's device simulates its own inputs at once, and when the server's state for an earlier tick arrives it rewinds what it predicts to that state and replays its inputs since. Because the two machines' floats can differ, the replayed state is close but not equal; the leftover error is smoothed away over a few frames instead of snapping. This needs:

1. Ticks: a numbered fixed step shared by server and players (physics already steps at a fixed 60 Hz).
2. Input streams per tick from each player (movement, look, Run, jump, and game-defined actions), sent redundantly on the unreliable channel.
3. Physics state save and restore for the predicted bodies (rapier's sets can be cloned), and stepping only those bodies again.
4. A server mode where characters are moved by the server from inputs (`Players.CharacterAuthority = "Server"`), next to today's client-moved characters (`"Client"`, the default, unchanged).

Stages: MP6a input-driven server characters with prediction and reconciliation; MP6b predicted physics bodies (owned assemblies, projectiles) through the same rewind; MP6c a Lua API for a game's own predicted state if real games ask for it. Each stage is measured (rollback cost per frame, correction size under emulated 100–200 ms and loss) before the next.

### D4. Validation: budgets per connection, checks per kind, strikes

- **Budgets.** Every message kind from a player gets a rate and size budget (token bucket), as Collaborate's `limits.rs` does: remote calls, chat, console lines, stream requests, owned-part packets. Going over drops the message and counts a strike. A RemoteEvent argument tree has limits on depth, table entries and bytes.
- **Movement.** `CharacterCheck` also refuses passing through anchored solid parts and Terrain (a ray from the last accepted place to the new one) and hovering above the ground for longer than a jump takes. Owned assemblies get the same speed check as characters, from their own velocity limits.
- **Strikes.** Each failed check is logged on the server with the player and why. A game reads them in Scripts (`Players.PlayerFlagged(player, reason)`) and decides; past a threshold per minute (a server setting, 0 off) the server kicks the player with a plain reason.

### D5. Operational tests run real processes

Operational tests start the real `axcenity-server` and real bot processes over UDP on one machine, inject the failure (kill the server, cut a bot's packets, flood), and check the outcome: players back in after a restart, memory and Instance counts back to where they started after churn, the server still at frame rate under a flood. Short versions run with `cargo test`; long soaks are `--ignored` and run by hand. Players' devices learn to **reconnect**: when the connection is lost (not when kicked or refused), the player tries again for 60 seconds with back-off, and the server takes the same UserId back in place of a stale connection.

### MP4: secure connections (cloud container, 2026-11-14)

[networking.md](../architecture/networking.md#secure-connections). Every server on the network is now secured unless it's a LAN game (`--host --lan`, `axcenity-server --insecure`, which logs a warning): a player gets a connect token over TLS from the token port (TCP, same number), checked against the fingerprint in a join link or the one remembered for the address, and netcode encrypts everything after. The certificate is kept in the engine's data folder (`server-certificate.json`), never a project; the token key is new each run.

| Test | Result |
|---|---|
| Join over UDP with the join link's fingerprint | In, encrypted (`Client::is_secured`); one token issued |
| A link with another fingerprint, or a remembered address whose certificate changed | Refused before the password is sent: "isn't the one you expected", with both short fingerprints |
| Wrong password, wrong version | Refused at the token port with the handshake's wording; no token |
| A connection without a token, and `--flood tokenless` (20 connections) beside 2 bots | Never answered; holds no slot; both bots join |
| A typed address with no token port (a LAN host) | Joined unsecured; with a fingerprint expected it fails with why instead |
| Hosting twice from one data folder | Same fingerprint |
| Every earlier UDP test (handshake, LAN search, rejoin after a crash, churn, floods, cheats) | Pass, now through tokens |

Choices made while building it: a player falls back to unsecured only when nothing answers on the token port and it has no fingerprint for the server, so a secured server can't silently be swapped for an unsecured one; the per-address limit (30 tokens a minute) doesn't apply to this computer's own addresses, so bots and the scale run can join 32 players from one machine. Breaking: an engine from before MP4 can't join a secured server.

Left: the PC–Mac join over Tailscale (a firewall must let TCP through as well as UDP on the game port); a join link in the player's UI and a "copy join link" for hosts (today the log prints it); NAT traversal is MP5.

## 4. What stays Paul's call

- Running a public `axcenity-relay` or any hosted service (the policy's rule 1; costs money).
- Whether `axcenity-server` on a public address may run unsecured at all (the plan: it may, with `--insecure` and a warning).
- Raising `Players.MaxPlayers`' default above 8 once the scale run shows how far it goes.
- MP6b and MP6c, after MP6a's measurements.

## 5. Phases

One PR per phase against `main`, opened in merge order. Each updates `docs/architecture/networking.md`, `wiki/Engine-Systems/Networking.md` and this file's status.

| Phase | What | Tested where |
|---|---|---|
| MP0 | This plan and ADR-0026 | — |
| MP1 | **Bots and the scale harness.** `axcenity-bots <address> --count <n>`: headless players as their own process over real UDP, each with a replica world, moving its character on a path and firing a RemoteEvent if the game has one; per-player round trip, join time, bytes and corrections, printed every few seconds and as a summary. `axcenity-server --stats <seconds>`: frame times and per-player bandwidth while serving. `scripts/net-scale.py` runs server and bots and writes a report. A test joins 32 bots over UDP on one machine. | Cloud (32 bots on one machine); then **Paul's PC and Mac** (16–32 bots split across them, LAN and Tailscale) for G13 |
| MP2 | **Operational tests and reconnecting.** Players reconnect after a lost connection; the server replaces a stale connection with the same UserId; tests for a server killed and restarted (saves kept, bots back in), churn (bots joining and leaving for minutes, no leak), floods (connection requests, oversized and many messages) and recovery. | Cloud |
| MP3 | **Validation.** D4's budgets, argument limits, wall and flight checks, owned-assembly speed, strikes, `Players.PlayerFlagged`, kick threshold. Tests with a "modified client" bot mode for each cheat. | Cloud |
| MP4 | **Secure connections.** D1: token port over TLS, the server's certificate kept in its data folder, connect tokens, encrypted netcode, fingerprints in join links and remembered per address, password at the token port, `--insecure`. | Cloud; then a PC–Mac join over Tailscale |
| MP5 | **NAT traversal and relay.** `axcenity-relay` (rendezvous, join codes, relaying), port mapping on the host, hole punching, fallback order and log. | Cloud (relay and NAT emulated with network namespaces where the container allows); **Paul's machines** for a real cross-network join |
| MP6a | **Prediction and rollback, stage 1.** D3's ticks, input streams, server-moved characters with prediction and reconciliation, error smoothing; measured under emulated latency and loss. | Cloud (loopback with emulated network); hand check on the PC |
| MP7 | **Scale run report.** G13's numbers from Paul's machines, with what they change (MaxPlayers default, budgets). | Paul's machines |

MP6b, MP6c and anything hosted wait for Paul.

## 6. Results

### MP1: 32 bots on one machine (cloud container, 2026-11-14)

`python3 scripts/net-scale.py run examples/hello-spawn --bots 32 --bot-processes 2 --seconds 60`, release build, the server and both bot processes on the same 4-vCPU container (about 2.5× slower than Paul's desktop, [benchmarks.md](../benchmarks.md)):

| | |
|---|---|
| Players at once | 32 (all joined; join p50 0.08 s) |
| Server frames a second at peak | 59 (lowest 5 s report) |
| Server frame time at peak | p95 7.2 ms; worst single frame 16.5 ms |
| Server sends | 1.02 MB/s in all, 32 KB/s per player |
| Each bot sends | 13.4 KB/s |
| Round trip | p50 20 ms, max 34 ms (loopback; renet's measure rounds to its tick) |
| Corrections | 0 |

**Found by the bots and fixed in MP1** (each has a test in `net`'s `character_tests`):

1. Characters spawned on one SpawnLocation stood in the same spot, pushed each other apart on their devices, and the server moved them back every frame (or they fell through the floor). Characters now take a free spot on the top (`players::free_spot`).
2. A character's first update had nothing saved up to spend, so its first step was refused.
3. A character that walked off the edge of a map fell further each update than the 30-stud teleport limit, and was moved back every frame from then on. Falling no longer counts toward the limit.

Before these, 32 walking bots were moved back about 87,500 times in a minute.

### MP2: crashes, churn and floods (cloud container, 2026-11-14)

Debug builds, the server and bots as separate processes over UDP on one 4-vCPU container (`crates/server/tests/operations.rs`, [networking.md](../architecture/networking.md#reconnecting)):

| Test | Result |
|---|---|
| Server `kill -9`, started again on the same port 2 s later | 3 of 3 bots rejoined (netcode's 15 s timeout, then 1–2 s of retries); each one's saved data came back (visit 1, then visit 2); 0 corrections |
| 4 bots churning every ~3 s for 16 s (about 20 joins and leaves) | The server's Instances back to where they started (21), Lua memory within 1.5× |
| 6 silent connections on a 2-player server (all 6 connection slots) | All dropped after 10 s; then 2 players joined |
| 3 junk join requests | Refused and dropped |
| 2 connections sending junk at renet's full rate (~3.5 MB/s each) | Server at 59 frames a second; 2 players joined in under 1 s and stayed in, but lost about 20% of their packets |

The plan said the server replaces a stale connection "with the same UserId". A UserId is whatever the device says, so anyone could knock another player out with it; the server instead gives each player a random rejoin key in `Welcome`, and only a Hello with that key replaces the old connection.

**Left for later phases:** a spamming connection is only dropped once MP3's per-connection budgets exist (the test notes the loss rather than failing on it); silent connections can hold every spare slot for 10 s until MP4's connect tokens keep strangers out (done: see MP4).

### MP3a: budgets, flags and kicks (cloud container, 2026-11-14)

[networking.md](../architecture/networking.md#validation). Found while building it: **a remote message of tables nested 200,000 deep (600 KB, under the reliable channel's 5 MiB) crashed any server or client that read it**, since postcard decodes nested tables recursively. Decoding now stops at 32 deep (`MAX_TABLE_DEPTH`), the sender raises an error past it, and a million deep (3 MB) is over the bytes budget anyway.

| Test | Result |
|---|---|
| Client firing a RemoteEvent 600 times a second | Held to the budget (600 at once, then 240 a second); flagged once a second; kicked at the 5th flag |
| `--flood deep` (one 600 KB message of nested tables a second) | Flagged unreadable or over budget; kicked in 5.5 s; server unharmed |
| `--flood spam` (2 connections at ~3.5 MB/s each) beside 2 walking players | Both kicked in under 3 s; players stayed in (at most a correction or so from the packets lost meanwhile); server above 50 frames a second |

Left: the junk a spammer sends before it's kicked still fills the server's socket for a moment; a larger socket receive buffer (needs `socket2`) would soften that.

### MP3b: walls, flight and strikes (cloud container, 2026-11-14)

[networking.md](../architecture/networking.md#validation). Found while building it: **two players spawned with no SpawnLocation stood at the same spot at the origin, and one was pushed into the floor and fell through it forever** (the old character test measured a falling character's walk). With the wall check the server held it back every frame; now characters at the origin spread out on rings as on a SpawnLocation. **Two more ways an honest character fell through the floor on its own device**, both found by the bots once the server refused it: a character pushed into another player's character, and one falling past a ledge and steered into its side below step height, had landing probes that started inside that part, found no ground and never landed. Now a falling foot inside a part lands on its top when that's within a step, and otherwise looks past it (`character::landing_hit`). With both fixed, 24 bots walked hello-spawn for 30 s without a refusal (six runs of seven).

| Test | Result |
|---|---|
| Modified client walking through a 1-stud wall at `WalkSpeed` | Never past the wall on the server; moved back; flagged "through a wall" |
| Modified client rising to 30 studs and hovering | Held under the top of a jump (about 10 studs over where it last stood); flagged "in the air" |
| Honest player walking into a wall, then jumping along it, 80 ms latency | No corrections, no flags |
| Owned part at 300 and 1,200 studs a second | 300 placed; 1,200 refused; flinging it on flags the player |
| `--cheat speed`, `noclip` (through the floor), `fly` bots beside 2 walking players over UDP | Each kicked for its own reason within 20 s; the players had no corrections |
| 24 honest bots on hello-spawn for 30 s, 7 runs | 6 runs with no refusals or corrections; one with 18 corrections and no flags, not traced (a respawn from the lava counts as a correction too) |

The checks cost a ray per update for a moving character (a second only when the first meets a wall), and a ray down plus a ball cast when it's off the ground, each a walk of the Workspace's bounding balls (`physics::query`). At 32 players and S10k's 10,000 parts that's a few thousand walks a second; MP7's run on Paul's PC measures it.

Left: a server Script that launches a character by moving its parts should give it a custom `Movement` while it flies; a moving anchored part that sweeps into a character on the server a moment before the device sees it moves can cost an honest player a correction.

