---
title: "ADR-0021: Collaborate: Tailscale connectivity, live shared scripts and an identity seam"
type: adr
adr: 21
adr_status: Accepted
date: 2026-11-12
supersedes: ADR-0020 (in part: decision D3, the TC8 hosted-service phase and the feature's name)
superseded_by:
affects: [collab, collab-server, editor, packaging]
source: [crates/collab/src/files.rs, crates/collab/src/access.rs, crates/collab/src/identity.rs, crates/collab/src/session.rs, crates/editor/src/app/team_panel.rs]
docs: [docs/research/collaborate.md, docs/research/team-create.md]
tags: [adr, networking, editor, collaboration]
---

# ADR-0021: Collaborate: Tailscale connectivity, live shared scripts and an identity seam

- **Status:** Accepted (Paul's brief "Axcenity Collaborate – Additional Architectural Decisions and Requirements", 2026-11-12). Supersedes [ADR-0020](ADR-0020-Team-Create-Session-Authority-And-Operations.md) in part; the rest of ADR-0020 stands.
- **Date:** 2026-11-12
- **Affected systems:** collab (documents, Tailscale, identity), collab-server, editor (script tabs, Collaborate panel and wizard), packaging (invite link handler)

## Context and problem

ADR-0020 built Team Create as one session authority ordering typed operations, with scripts edited by one person at a time (decision D3), reachability left to the team (LAN, a forwarded port, or a VPN they already have), and a hosted account service planned as TC8. Paul's 2026-11-12 brief keeps the host-authoritative model and asks for three different directions: character-level shared script editing with a CRDT, Tailscale as the guided way to connect with a five-minute setup, and no hosted or Django account service now, only abstractions ready for one. It names the feature Collaborate. The audit is `docs/research/collaborate.md`.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Keep script locks (ADR-0020 D3) | Built and tested | The brief rules it out: only one person can type in a script |
| **CRDT documents for Lua scripts and stylesheets (`yrs`), relayed and checked by the host** | Concurrent typing with automatic merges, offline edits merge, cursors, own-edit undo; the host still refuses updates a member may not send | A new dependency (three new crates in the tree); text tabs must map edits to inserts and deletes |
| CRDT for every document (block scripts, graphs, materials) | One model | Structured documents would need their own CRDT schemas; locks work for them and the brief asks only for scripts |
| **Tailscale through its installed client's CLI** | Users install a known app; no network stack of ours; MagicDNS names in invites | A third-party service; cross-tailnet use needs a one-time share in Tailscale's admin console |
| Embed Tailscale (`tsnet`) | No separate install | Go only; a large native dependency |
| Our own relay | No third party | A hosted service, which the brief and the development policy rule out |

## Decision

1. **Scripts are live shared documents.** Lua scripts and stylesheets are `yrs` documents; members send updates, the host checks capability, size and that the update decodes, applies it and passes it on, and writes the text back to the plain `.lua` file. Other documents keep ADR-0020's soft lock. This supersedes ADR-0020's D3 for those two kinds.
2. **Tailscale is the supported way to connect.** Axcenity reads and drives the installed Tailscale client (`tailscale status --json`, `up`, `whois`), invites name the host by its MagicDNS name, and an in-editor wizard covers install, sign-in, sharing and a connection test. Tailscale provides the network path only: membership, roles and the signed handshake stay Collaborate's own. LAN and forwarded ports keep working.
3. **No hosted service.** ADR-0020's TC8 (Axcenity accounts and a hosted service) is replaced by an `IdentityProvider` seam with today's device keys as its only implementation, an optional account link in the members file, and a document describing the future website integration. Nothing is deployed.
4. **The feature is called Collaborate** in everything users see. "Team Create" is Roblox's product name. Old invite links and `--team-*` flags keep working; internal identifiers are renamed only when their files are next changed.

## Reasons

See `docs/research/collaborate.md` §2–§6.

## Consequences

- `collab` gains documents (`yrs`), a Tailscale module and an identity provider trait; the wire gains message kinds for documents and chat.
- Script tabs in a session no longer show "Alex is editing"; conflict copies remain only for structured documents and for files changed outside a session.
- The five-minute target depends on Tailscale's sign-in and sharing pages, which Axcenity can open but not automate.
- Revisit if Tailscale's terms or sharing model change, if a hosted service becomes worth running, or when the Django website exists (then add a second `IdentityProvider`).

## Implementation references

- Audit and plan: `docs/research/collaborate.md` (phases C1–C12)
- ADR-0020's built system: `crates/collab`, `crates/collab-server`, `crates/editor/src/app/team.rs`
- Nothing of this ADR is implemented yet (C1 is the audit).
