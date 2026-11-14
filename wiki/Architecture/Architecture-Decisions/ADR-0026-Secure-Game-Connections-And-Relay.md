---
title: "ADR-0026: Secure game connections through self-issued connect tokens, and a self-hostable relay"
type: adr
adr: 26
adr_status: Accepted
date: 2026-11-14
supersedes:
superseded_by:
affects: [net, server, player]
source: [crates/net/src/transport.rs]
docs: [docs/research/multiplayer-readiness.md, docs/architecture/networking.md]
tags: [adr, networking, security]
---

# ADR-0026: Secure game connections through self-issued connect tokens, and a self-hostable relay

- **Status:** Accepted (Paul asked for secure connections and NAT traversal on 2026-11-14; the design is the plan's D1 and D2 and can be changed by him)
- **Date:** 2026-11-14
- **Affected systems:** the network transport, `axcenity-server`, the player's `--host` and `--join`, `NetworkService`

## Context and problem

Games connect with `renet_netcode` in its unsecured mode (`crates/net/src/transport.rs`): packets are neither encrypted nor authenticated, and the join password crosses the network in the clear. That was enough for LAN and direct-address play, the only scope the development policy allowed ([ADR-0010](ADR-0010-Listen-Server-Replicas-And-Network-IDs.md), M11 deferred). Paul now wants games ready for public hosting. Netcode's secure mode encrypts every packet with keys carried in a **connect token** that the player must receive privately; netcode leaves who issues tokens to the game. There is no account service (R19 is design only), and the policy rules out hosted infrastructure until it's needed. Most home hosts are also behind a router that drops unsolicited UDP.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| The game server issues tokens itself over TLS, pinned by its certificate's fingerprint | No back end; same trust model as Collaborate's links (ADR-0021); works for listen and dedicated servers | A typed address trusts the first certificate it sees |
| A web token service with accounts | netcode's usual setup; accounts decide who joins | Infrastructure and accounts that don't exist |
| Axcenity's own encryption over unsecured netcode | No token step | Hand-rolled cryptography where netcode's is proven |
| Relay only through Tailscale or another VPN | Nothing to build | Every player must install and join the same network |

## Decision

A server on the network issues its own connect tokens: a player opens TLS to the server's token port (TCP, same number as the UDP game port), is checked (password, game, version), and receives a 30-second connect token; netcode then connects in secure mode. The player checks the server by its certificate fingerprint from a join link, or remembers the first one per address. LAN play, editor Play and the loopback stay unsecured. Reaching a host goes direct, then router port mapping, then hole punching, then a relay; rendezvous and relay are one self-hostable binary, `axcenity-relay`, and the engine runs none for other people.

## Reasons

- Netcode's secure mode is the vetted part; Axcenity only adds the token hand-off, over rustls, which Collaborate already relies on.
- No service has to exist for a game to be hosted safely. If accounts come (R19), an account service can issue the same netcode tokens with the server's key.
- The relay can't read or alter a game: the encryption is between player and server.

## Consequences

- Two ports per server (UDP and TCP with the same number) to forward or map.
- The network protocol changes: an older player can't join a secured server, and the refusal must still be readable (the token port answers in JSON, as `Hello` does).
- A public relay costs money to run; that's Paul's decision, not the engine's.
- Revisit if an account system is built, or if TLS's extra round trips make joins noticeably slower over long distances.

## Implementation references

- Plan: `docs/research/multiplayer-readiness.md` phases MP4 (secure connections) and MP5 (NAT and relay)
- `crates/net/src/secure.rs` (MP4: `ServerCert`, the token port, `fetch_token`, `Trust`, join links, `known-servers.json`) and `secure_tests.rs`
- `crates/net/src/transport.rs` (`Server::listen_secure`, `Client::join`: a token first, then encrypted netcode; unsecured only for a LAN game)
- `crates/net/src/game.rs` (`Network::set_secure`, `set_data_folder`, `fingerprint`); `axcenity-player --host --lan`, `axcenity-server --insecure` and `--data`
- MP5a: `crates/relay` (`axcenity-relay`: join codes, rendezvous, UDP relaying, TCP splicing; `protocol.rs` shared with `net`), `crates/net/src/relayed.rs` (the host's `RelayLink`, a player's `fetch_by_code` with hole punching, `Route`), `crates/net/src/host_socket.rs` (the host's netcode server with relayed players), `scripts/nat-lab.sh`
- One refinement in building it: tokens list `0.0.0.0:<port>` beside the address the player asked for, since a server behind a router doesn't know its public address; the per-run token key ties a token to one server
