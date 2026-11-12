---
title: "ADR-0022: Persistent Collaborate projects: Git sync and automatic host handoff"
type: adr
adr: 22
adr_status: Accepted
date: 2026-11-12
supersedes: 
superseded_by:
affects: [collab, editor]
source: [crates/collab/src/session.rs, crates/collab/src/access.rs, crates/collab/src/hosted.rs, crates/collab/src/host.rs, crates/collab/src/client.rs]
docs: [docs/research/collaborate-persistent.md, docs/research/collaborate.md]
tags: [adr, networking, editor, collaboration]
---

# ADR-0022: Persistent Collaborate projects: Git sync and automatic host handoff

- **Status:** Accepted (Paul's brief "Axcenity Collaborate 2.0 – Persistent Collaboration & Automatic Host Handoff", 2026-11-12). Extends [ADR-0020](ADR-0020-Team-Create-Session-Authority-And-Operations.md) and [ADR-0021](ADR-0021-Collaborate-Tailscale-Live-Scripts-And-Identity-Seam.md); supersedes neither.
- **Date:** 2026-11-12
- **Affected systems:** collab (project identity, members list, election, handoff), a new `repo` crate (Git and GitHub), editor (Collaborate panel)

## Context and problem

Collaborate has one session authority, the host (ADR-0020). Every member holds a full copy of the World, the files and the live scripts, but only the host holds the order of edits, the members list, the journal, the checkpoints and the network endpoint pinned in invites. When the host leaves, the session ends for everyone; when it crashes, everyone waits for that computer to come back. Paul's brief asks for projects that outlive their host: another member takes over a live session, and a project can be resumed from a Git remote when nobody is online. The audit is `docs/research/collaborate-persistent.md` §2.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| **Movable authority: a deterministic successor takes over as a new epoch from its own copy** | Reuses epochs, re-sends and state-vector merges built in TC6/C7; no new service; one host at a time keeps ADR-0020's model | Needs the members list and would-be hosts' addresses replicated; a crashed host's last broadcasts need a tail kept by members |
| Leaderless sync (CRDT for the whole World) | No host to lose | Rewrites working sync; loses host-side validation and the conflict rules ADR-0020 chose |
| A permanently running relay or dedicated server | Simple failover | The brief and the development policy rule out a required server; `axcenity-collab` stays optional |
| Git as the live channel | One mechanism | Git isn't real-time; the brief rules it out |
| A Git library (`gix`, `git2`) | No external tool | The brief asks for established Git; users' credential helpers, LFS and hooks work with the installed `git` |

## Decision

1. **Authority moves; ownership doesn't.** The session host is replaceable. Project ownership is the Owner role in a members list that the host shares with every member who may host. A successor hosts with that list, so roles and the Owner never change in a handoff.
2. **Deterministic succession, no voting.** The host publishes an ordered successor list (named backup host, then longest-connected eligible member, then lowest member ID) with each candidate's address and certificate fingerprint. When the host leaves (announced) or is silent past a grace period, every member picks the first reachable successor from the same list.
3. **The successor starts from its own copy as a new epoch.** Members reconnect, get a snapshot and re-send their unconfirmed edits (TC6); live documents merge by state vector (C7). After a crash, the successor first collects transactions some members applied and it didn't, from a bounded tail each member keeps; anything unrecoverable is reported, never guessed.
4. **A project has a persistent identity** in a non-secret `collaborate.json` (project ID, name, optional remote) that syncs and is committed; each computer keeps a record of its projects for rejoining without an invite.
5. **Git is optional persistence.** A project may link to a Git remote (GitHub first, through the installed `git`, OAuth device sign-in, tokens in Git's own credential store, which is the OS store on usual installs). Only the current host commits during a session; automatic checkpoints are batched, never force-pushed; a merge git can't make cleanly stops and keeps both versions until someone chooses.

## Reasons

See `docs/research/collaborate-persistent.md` §1–§4.

## Consequences

- `collab` gains `project`, `election` and `handoff` modules and message kinds for members lists, host cards, successors, handoff and the transaction tail (appended after C10's).
- A new `repo` crate wraps `git` and GitHub; `collab` doesn't depend on it.
- Members' editors can become hosts: the Windows firewall prompt can appear for anyone eligible, so a member is eligible only after its editor has listened once.
- Two hosts can exist after a network split; epochs and Git conflict handling are the backstop.
- Revisit if a hosted Axcenity service is ever accepted (a relay could then replace election), or if Tailscale offers stable shared endpoints.

## Implementation references

- Design and phases P0–P7: `docs/research/collaborate-persistent.md`
- Built on: `crates/collab` (`host.rs`, `client.rs`, `session.rs`, `access.rs`, `hosted.rs`)
- Nothing of this ADR is implemented yet (P0 is the design).
