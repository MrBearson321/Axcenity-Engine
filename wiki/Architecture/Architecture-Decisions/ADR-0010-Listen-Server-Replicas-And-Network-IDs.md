---
title: "ADR-0010: Server-authoritative replicas with shared IDs; single-player is a listen server"
type: adr
adr: 10
adr_status: Accepted
date: 2026-10-29
affects: [net, server, script, core, editor, player]
source: [crates/net/src/replicate.rs, crates/net/src/motion.rs, crates/net/src/handshake.rs, crates/net/src/wire.rs, crates/core/src/arena.rs]
docs: [docs/architecture/networking.md, docs/development-policy.md]
tags: [adr, networking]
---

# ADR-0010: Server-authoritative replicas with shared IDs; single-player is a listen server

- **Status:** Accepted (Milestone 14, Phases 63–67, decided 2026-10-29; wire format changed to postcard by M7, 2026-11-10; secure netcode deferred by policy)
- **Affected systems:** networking, scripting sides, the world's ID space, editor Play

## Context and problem

Roblox-style games need a server world and client views, RemoteEvents between them, and the same script code for single-player and multiplayer. Every example had to keep playing exactly as before.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Lockstep | Tiny bandwidth | Needs cross-machine determinism (not a goal) |
| Separate single-player path | Simple single-player | Two behaviors; "works alone, breaks online" |
| **Server-authoritative replication, replicas at the server's IDs, single-player and Play as a listen server** | One code path; IDs are the same number on every device; RemoteEvents work identically over loopback and network | Bandwidth and diff cost (addressed by M7) |

## Decision

Each device has its own World; a joined device keeps a replica whose Instances have the server's IDs (`World::create_at`) and numbers its own from 2³¹. The server diffs and sends reliable updates and budgeted, quantized motion; players own their character and assigned assemblies, checked by the server. Single-player and editor Play are listen servers (chosen 2026-10-29; `dump_every_example` showed identical output for all 55 examples before and after). Wire: postcard since M7 (`Hello`/`Refused` stay JSON forever). Netcode ran unsecured until multiplayer readiness MP4 secured servers on the network ([ADR-0026](ADR-0026-Secure-Game-Connections-And-Relay.md)).

## Consequences

- `PROTOCOL_VERSION` rises when a message or class default changes.
- Server-only services never leave the server.
- Not safe for public internet hosting until M11.

## Implementation references

`crates/net`; [Networking](../../Engine-Systems/Networking.md).
