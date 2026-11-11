---
title: "ADR-0020: Team Create uses one session authority and host-ordered operations"
type: adr
adr: 20
adr_status: Proposed
date: 2026-11-11
affects: [editor, core, net, project]
source: [crates/editor/src/history.rs, crates/core/src/arena.rs, crates/core/src/world.rs, crates/net/src/handshake.rs]
docs: [docs/research/team-create.md]
tags: [adr, networking, editor, collaboration]
---

# ADR-0020: Team Create uses one session authority and host-ordered operations

- **Status:** Proposed (waiting for Paul's decisions D1–D5 in the proposal)
- **Date:** 2026-11-11
- **Affected systems:** editor (undo history, panels), core (arena ID allocation), net (wire encoding, handshake checks), project (saving)

## Context and problem

Paul's brief (2026-11-11, section 8) asks for Team Create: several people editing one project at once, with accounts, invitations, roles, real-time sync, conflict handling and security, and asks for the architecture first. There is no account system and no hosted service, and the development policy (`docs/development-policy.md`) defers internet hosting (M11). Every editor edit already goes through `History` with exact before and after values, and replicas already share the server's Instance IDs ([ADR-0010](ADR-0010-Listen-Server-Replicas-And-Network-IDs.md)).

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Whole-file sync | Trivial | Loses concurrent work; slow for big worlds; the brief rules it out |
| CRDT on every peer | No server | Tree moves and arena IDs are hard to converge; permissions need a trusted place anyway |
| Hosted backend only | No setup for users | Infrastructure, cost and personal data before they're needed |
| **One session authority (in the owner's editor, a self-hosted binary, or later a hosted service) ordering typed operations** | One order, permissions in one place, no infrastructure now, maps onto `History` steps | The in-editor host ends the session when the owner leaves; internet reachability needs a forwarded port, an overlay network or a later relay |

## Decision

A Team Create session has one authority that orders transactions of typed operations (create subtree, destroy, reparent, set properties, attributes, tags, terrain chunks, string cells, file versions). Clients apply their own edits at once and reconcile by the host's order (last writer wins per property; delete wins; the host refuses tree cycles). Each undo step is one transaction. New Instances take IDs from host-assigned blocks so `InstanceId` stays the network ID. Identity is a per-machine key pair with signed invites until an Axcenity account service exists.

## Reasons

See `docs/research/team-create.md` §4–§7.

## Consequences

- Extends ADR-0010 from one writer to several: the arena gains allocation from host-given ranges in collaborative Worlds.
- Every editor change must be a recorded `History` step; a change-log check makes a bypass a failing test.
- The project folder stays the ADR-0016 text format; membership and keys stay outside it.
- Revisit when internet hosting (M11) or Axcenity accounts arrive (proposal phase TC8).

## Implementation references

- Proposal and phased plan: `docs/research/team-create.md`
- Nothing implemented yet.
