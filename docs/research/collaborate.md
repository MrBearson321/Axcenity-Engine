# Collaborate: audit of Team Create and the plan for Paul's 2026-11-12 brief

Paul's brief "Axcenity Collaborate – Additional Architectural Decisions and Requirements" (2026-11-12) sets the direction for **Collaborate**, the feature built so far as Team Create (TC1–TC7, [team-create.md](team-create.md), [ADR-0020](../../wiki/Architecture/Architecture-Decisions/ADR-0020-Team-Create-Session-Authority-And-Operations.md)). Its first priority is an audit of what exists and what can be reused. This is that audit, the decisions it leads to ([ADR-0021](../../wiki/Architecture/Architecture-Decisions/ADR-0021-Collaborate-Tailscale-Live-Scripts-And-Identity-Seam.md)) and the phased plan C1–C12, one phase per priority in the brief.

- **Status:** C1 (this audit) done. C2 onwards follow the plan in [§8](#8-phased-plan-c1c12).
- **Date:** 2026-11-12
- **Engine:** `main` at `d45d70b` (PR #195, the LAN test fixes)
- **Relies on:** ADR-0020 (one session authority, host-ordered operations), ADR-0010 (shared IDs), ADR-0016 (text project format), ADR-0008 (per-machine cooked cache), the [development policy](../development-policy.md) (no infrastructure before it's needed).

## 1. Summary

- **The brief's architecture is already the one built.** Team Create is host-authoritative: one host holds the authoritative World, checks every member's role, orders typed operations, sends only what changed, keeps a journal, recovers after a crash and never lets a reconnecting member overwrite newer work. About 9,000 lines of sync, network, file and reliability code (`crates/collab`, `crates/collab-server`, `crates/editor/src/app/team*.rs`) carry over as they are. No working code is rewritten.
- **Three things change direction**, recorded in ADR-0021, which supersedes ADR-0020 in part:
  1. **Scripts become live shared documents (CRDT, `yrs`)** instead of one editor at a time with a lock (ADR-0020's D3). Block scripts, graphs, materials and other structured documents keep the lock.
  2. **Tailscale is how people reach each other**, set up by an in-editor wizard, instead of "same LAN, a forwarded port, or a VPN you already have". Tailscale gives the network path only; Collaborate's own keys, invites and roles still decide who may edit.
  3. **The feature is called Collaborate.** "Team Create" is the name of Roblox's feature, and the repo's rules forbid Roblox branding. Code names (`collab`, `axcenity-collab`) already fit; user-facing text, docs, flags and the invite link change, and the old link and flags keep working.
- **New pieces the brief needs:** the setup wizard and Tailscale integration, an invite link the OS opens in Axcenity, chat, a host-side revision history (checkpoints on the host, browsable by members), an `IdentityProvider` seam with only today's device keys behind it, and documentation for all of it. Nothing needs an Axcenity server, a website or the Django account manager.

## 2. Requirement by requirement

Verdicts: **Reuse** (works as the brief asks), **Extend** (exists, needs more), **New**, **Change** (works but the brief chose differently; see ADR-0021).

### 2.1 Host-authoritative project synchronization (brief §1)

| Requirement | What exists | Verdict |
|---|---|---|
| Host holds the authoritative scene | `collab::Host` (`host.rs`) owns the session World, in the hosting editor or in `axcenity-collab` (`collab::hosted` shares the setup) | Reuse |
| Process and validate incoming edits | `apply.rs`: all-or-nothing transactions, known classes and property types, tree rules (no cycles, no writes into deleted Instances), size limits; refusals carry the reason and the current state | Reuse |
| Broadcast accepted changes | `ToClient::Accepted` with the host's sequence number; the author's copy doubles as the acknowledgement | Reuse |
| Collaborator permissions | `role.rs` (capability sets: Owner, Editor, Viewer), checked by the host on every operation, file save and lock | Reuse; more roles are data |
| Membership and connection status | `access.rs` (members by public key, invites), `presence.rs` (roster: online, idle, offline), the panel's connection chip | Reuse |
| Checkpoints and revision history | The host's journal (`journal.rs`) is crash recovery only and is emptied by each snapshot. Version History (`project/src/versions.rs`) is per machine and is never written by a session | **Extend** (C9) |
| Resolve conflicting scene edits | Last writer per property in host order, delete wins with Restore, cycle refusal, create under a deleted parent goes to the nearest survivor (`docs/research/team-create.md` §8, one test per rule in `tests.rs`) | Reuse |
| Clients keep a local working copy and get increments | Each editor works on its own World and applies others' transactions; joining sends one snapshot, then only transactions | Reuse |
| Never send the whole project per change | Per-frame capture of the World's edit journal (`capture.rs`) into one small transaction; files go by SHA-256 once | Reuse |
| Batching and serialization | One transaction per frame or per gesture (drags at most 20 a second), postcard on the wire behind a format byte (`wire.rs`) | Reuse; compression for big messages in C11 |

### 2.2 Content-specific synchronization (brief's table)

| Content | Brief | Today | Verdict |
|---|---|---|---|
| Scene objects and transforms | Host-authoritative operations | `CreateTree`, `Destroy`, `Reparent`, `SetProps` ordered by the host | Reuse |
| Components and properties | Validated incremental updates | `SetProps`, `SetAttribute`, `Tag`, checked against the class's property types | Reuse |
| Scripts | CRDT documents | Whole-file versions with compare-and-swap and a soft lock per file (`files.rs`, `mirror.rs`); a lost race leaves a `.conflict-<user>` copy | **Change** (C7) |
| Asset imports and modifications | Versioned asset sync | Content-addressed files: upload by hash, then `FileVersion { path, base, new }` with compare-and-swap; an import and its placed model are one transaction | Reuse |
| Project settings | Host-authoritative changes | Settings files (`project.json`, `input.json`, `strings.json`) sync as files with compare-and-swap; changing them needs the Owner's capability | Reuse; a settings conflict shows a copy rather than merging per key (fine for v1) |
| Chat and presence | Lightweight real-time messages | Presence (selection, camera, field being edited; at most 10 a second, never journaled). No chat | Presence: Reuse. Chat: **New** (C8) |
| Version history | Persistent revisions and checkpoints | See §2.1 | **Extend** (C9) |

### 2.3 Offline recovery and persistence (brief §1)

| Requirement | What exists | Verdict |
|---|---|---|
| Host keeps persistent state | Journal written before each broadcast; snapshot every 30 s with changes; `axcenity-collab` also writes `world.json` every 30 s and on `quit` | Reuse |
| Clients keep enough to recover | Unconfirmed edits in `.axcenity/teamcreate/unsent`, re-sent on the next join (`Client::recover_unsent`) | Reuse |
| Operation sequencing and identifiers | `client_seq` per member, host sequence numbers, host epochs per run | Reuse |
| Duplicate-event protection | Host drops a `client_seq` it already applied; clients apply strictly in order and resync on a gap | Reuse; the rare double-apply when a host stops right after writing an edit (team-create.md TC6 "Left") is fixed in C5 |
| Reconnection and reconciliation | Reconnect with backoff (1 s to 30 s), catch up from the journal or a fresh snapshot, remap IDs from an old run (`op::remap_ids`) | Reuse |
| Unsynchronized change detection | The status chip counts waiting changes; the World's edit journal means nothing can change the World without being captured | Reuse |
| Automatic recovery checkpoints | None on the host or in members' mirrors | **New** (C5, C9) |
| Incompatible project states | Handshake refuses another protocol, engine version, build or class-defaults fingerprint with a sentence; epochs detect a host that started again | Reuse |
| Clear behavior when the host crashes | Members keep editing offline; a restarted host recovers from the journal and members reconnect; a host that *ended* makes copies read-only with "The host left the session" | Reuse |
| Never silently overwrite newer data on reconnect | World: re-sent edits go through the host's normal checks, and any refused or partly applied ones are listed in **Recovered Changes**. Files: compare-and-swap, conflict copies | Reuse |
| Offer a recovery or reconciliation process | Recovered Changes lists what didn't apply but can't re-apply it; there's no "compare my copy with the session" view | **Extend** (C5) |

### 2.4 Setup and connectivity (brief §2)

| Requirement | What exists | Verdict |
|---|---|---|
| Guided first-time wizard | None. The panel's empty state has Share and Join | **New** (C4) |
| Detect, install, sign in to Tailscale | Nothing Tailscale-aware; the panel mentions "a VPN such as Tailscale" | **New** (C3) |
| No IP addresses or ports for users | Invites already carry the address and the host certificate fingerprint, so joiners never type one. The address is the LAN address (`session::lan_address`), which is wrong across networks | **Extend** (C3: the host's Tailscale name) |
| Start Session with name and permissions | Share hosts at once with port 47810 and makes an Editor invite; the role and lifetime come from Settings | **Extend** (C4: one Start Session dialog) |
| Invitation link or code | `axcenity://teamcreate/join?host=…&cert=…&invite=…`, single use, 7 days by default, revocable | Reuse; **New**: the OS opening the link in Axcenity (scheme registration in `packaging/`) and a short code to read aloud |
| Host sees status, users, invite, Tailscale, permissions | Panel has status, roster with roles, invites | **Extend** (Tailscale line) |
| Contextual help and a setup guide with pictures | `documentation/learn/projects.md` has one Team Create section | **New** (C4) |
| Measure onboarding time | Nothing | **New** (C4: the wizard times each step locally; nothing is sent anywhere) |

### 2.5 Identity and the future Django account manager (brief §3)

| Requirement | What exists | Verdict |
|---|---|---|
| Works with no website or account service | Device key pair per machine (`identity.rs`, Ed25519) and signed invites; nothing hosted | Reuse |
| Authorization separate from connectivity | Membership is by public key on the host (`access.rs`); being able to reach the host gets you only a handshake | Reuse; Tailscale changes nothing here |
| Display names, basic identification | Name, colour and fingerprint in Settings > Team Create | Reuse |
| Identity interface independent of the provider | Code uses `Identity` and `PublicKey` directly | **Extend** (C10/C12: an `IdentityProvider` seam with one implementation) |
| Session identity linkable to an account later | Members are keyed by device key only | **Extend** (C12: room for an optional account link in the members file, unused) |
| No Django backend or placeholder service | None exists | Keep it that way |

## 3. Where the brief and the built system differ

| Topic | Built (ADR-0020) | Brief | Resolution |
|---|---|---|---|
| Script editing | D3: one editor per document, soft lock, whole-file versions | CRDT, character-level, cursors, automatic merges | ADR-0021 supersedes D3 for Lua scripts and stylesheets only. Structured documents keep locks |
| Reachability | LAN, forwarded port or a VPN the team already has | Tailscale, guided, no networking knowledge | ADR-0021: Tailscale is the supported path; LAN keeps working |
| Name | Team Create | Collaborate | ADR-0021: rename user-facing names, keep old links and flags working |
| Accounts | TC8 "accounts and a hosted service", deferred | No hosted or Django service now; design the seam, document it | TC8 is replaced by C12 (document and seam only) |
| Hosting | Hybrid: in the editor or `axcenity-collab`; a hosted service later | Self-hosted, no Axcenity servers | Same, minus the hosted service |

Nothing in the brief conflicts with host-authoritative operations, ID blocks, the journal, roles or file sync.

## 4. Live scripts (C7)

`yrs` 0.28 (MIT, the Rust port of Yjs, maintained by the y-crdt project) is the brief's suggestion and fits:

- **One `yrs::Doc` per open Lua script or stylesheet**, a single `Text` in it. Clients send `yrs` v1 updates; the host checks the member may edit scripts, decodes the update (refusing one that doesn't decode or is too big), applies it to its copy and passes it on. Order no longer matters for text, but the host still sees and can refuse every change, so the session stays host-authoritative.
- **Files stay plain `.lua`.** The host writes the document's text to the file through the existing `FileVersion` path when edits settle (and on save), so git, the watcher, Play and `axcenity-collab`'s saves are unchanged. The host keeps the document's `yrs` state in its data folder so a restart keeps the merge history.
- **Joining or reconnecting** sends the member's state vector; the host replies with only what's missing. Edits made offline merge when they arrive: nobody's text overwrites anybody's.
- **Edits from outside the session** (a file changed in VS Code on the host) become a text diff applied to the document, like a member's edit.
- **The editor** turns each frame's `TextEdit` change into inserts and deletes on the document and applies incoming updates while keeping the cursor in place (`yrs` sticky indexes). Others' cursors and selections travel in presence. Undo uses `yrs`'s `UndoManager` limited to your own edits, matching Team Create's "undo only your own steps".
- **Cost:** three crates new to the tree (`arc-swap`, `dashmap`, `smallstr`); `async-lock`, `async-trait`, `fastrand`, `smallvec`, `serde`, `thiserror` are already in `Cargo.lock`. Licenses are MIT or Apache-2.0/MIT.

Alternatives considered: `automerge` (heavier, more than text), `loro` (younger API), `diamond-types` (text only and fast, but no maintained sticky-index or undo story), and keeping locks (rejected by the brief).

## 5. Tailscale and the five-minute setup (C3, C4)

### 5.1 How Axcenity talks to Tailscale

Through the installed Tailscale client's command-line tool, never an embedded network stack (Tailscale's embeddable library `tsnet` is Go only). A new `collab::tailscale` module (no UI) does:

- **Find it**: `tailscale` on the PATH, else the standard install locations (`C:\Program Files\Tailscale\tailscale.exe`; `/Applications/Tailscale.app/Contents/MacOS/Tailscale`; `/usr/bin/tailscale`). Missing means the wizard offers the download page for the platform.
- **Read its state**: `tailscale status --json` gives the backend state (`NeedsLogin`, `Stopped`, `Starting`, `Running`), this machine's MagicDNS name and `100.x` addresses, the tailnet name and the signed-in user. Parsed into a small `TailscaleState` enum, tested with recorded JSON in the cloud.
- **Sign in**: `tailscale up` (or `login`) prints the sign-in URL, which the wizard opens in the browser; the wizard polls `status` until `Running` and then says so.
- **Check the path**: before the handshake, a TCP connect to the host's Tailscale name, with a plain-words result ("Your computer can't see Paul-PC on Tailscale yet. Ask Paul to share it with you").
- **Show who's who**: `tailscale whois` on a member's address shows their Tailscale login beside their Collaborate name in the roster. It is shown, never trusted for permissions.

The invite names the host by its MagicDNS name (falling back to its `100.x` address) instead of the LAN address, so links work from anywhere the joiner's Tailscale can reach.

### 5.2 What can stop five minutes

| Step | Typical time | Risk | Mitigation |
|---|---|---|---|
| Install Tailscale | 1–2 min | Needs admin rights on Windows and macOS | The wizard says so up front; installed once |
| Create a Tailscale account | 1 min | Uses a Google, Microsoft, GitHub or Apple login (Tailscale has no own passwords) | Explain in one sentence; opens the browser and comes back |
| Reach the host's machine | 1–2 min the first time | Joiner and host are in **different tailnets** (the normal case for friends) | Tailscale's per-machine **sharing**, which Tailscale itself recommends for a private game server: the host shares their machine once (admin console, Machines › … › Share, by email or by a link that can be reused up to 1,000 times and expires unused after 30 days; there's no CLI for it) and the joiner accepts it with their own Tailscale account. A shared machine takes incoming connections but can't start any, which is exactly the host's role; it's free on every plan and doesn't add to the user count. The joiner reaches it only by the full name (`<machine>.<tailnet>.ts.net`), which is what invites carry. Inviting people into the host's tailnet also works and suits a standing team |
| Organization approval | unbounded | A company tailnet may require device or user approval | Detected (`status` shows the machine waiting); the wizard says who has to approve |
| Windows firewall | 10 s or blocked | The first listen on port 47810 raises the Windows Defender prompt; a non-admin account can't allow it | Being checked in C3 (see §5.3) |
| Every later session | under 1 min | none expected | State remembered; Start Session and the invite link are one click each |

### 5.3 Open questions C3 settles with tests on Paul's PC and Mac

- **The Windows firewall.** Tailscale's own filter rules let tunnel traffic in, but whether an app listening on port 47810 still needs a Windows Defender allow rule for tailnet peers isn't documented. Plan A: listen as today and let the first-run wizard explain the one-time Windows prompt (the installer can add the rule, since it runs as administrator). Plan B, `tailscale serve` forwarding a tailnet port to `127.0.0.1:47810`, avoids the firewall but makes the tailnet's admin turn on HTTPS certificates first (Tailscale's consent page), so it's a fallback, not the default.
- That a shared machine's non-HTTP port (47810) is reachable by the person it's shared with, by its full MagicDNS name.
- What `tailscale status --json` and `tailscale up --json` say on each platform (the macOS App Store client's CLI lives inside the app), recorded as test fixtures.
- Whether accepting a share needs the joiner to be the Owner or an Admin of their own tailnet (Tailscale's docs say so; true for anyone with a personal tailnet, a blocker inside a company tailnet the wizard must explain).

## 6. Identity, authorization and the Django account manager (C10, C12)

Today: an `Identity` (name, colour, Ed25519 key) per machine, a members file on the host mapping public keys to roles, and invites that bind a new key to a role. Tailscale adds a *network* identity (the tailnet login) that is shown but never grants anything; the brief's "being on the tailnet doesn't mean you can edit" is already how the host works.

Seams to add (no new service, no placeholder backend):

- `IdentityProvider`: makes and loads the local identity and signs the handshake challenge. One implementation now (`DeviceKeys`). A later `AxcenityAccount` provider would sign in through the website (OpenID Connect with short-lived tokens the host verifies against the site's published keys).
- **Members keyed by a principal, not a key.** The members file gains an optional `account` field next to the key, unused now. A future account can then claim existing device-key memberships (the host asks the Owner to confirm), so ownership, roles and the journal's attribution carry over; one account may own several device keys and a device key may be linked to no account, so the mapping is many-to-one, never assumed one-to-one with Tailscale logins.
- **Versioned integration points** listed in C12's document (sign-in, token refresh, profile, invite by account name, project ownership) with the website's API version in each call. Nothing calls them until the website exists.

## 7. Other gaps found

- **Invite links don't open Axcenity.** No `axcenity://` handler is registered by the Windows installer, the macOS bundle or the Linux desktop file, and a second editor launch doesn't hand the link to the running one. C4.
- **No LAN discovery for sessions** (games have it). Not needed once Tailscale is the path; skipped.
- **Version History checkpoints of a member's mirror** were left from TC6. C9.
- **UI snapshots of the panel** were taken on Paul's PC in the hardware test; the wizard needs its own. C4.
- **The double apply after a host stops** right after writing an edit (TC6 "Left"). C5.

## 8. Phased plan C1–C12

One phase per priority in the brief, in order. Each is one or more PRs to `main`, tested locally (cloud first; Paul's PC and Mac only where the step needs real Tailscale or two machines, sequenced by the coordinator). Every phase writes its failing tests first (RED), then the code (GREEN), and updates this document, the topic docs and the wiki in the same PR.

| Phase | Builds | Tests (RED first) | Security | Performance | Done when |
|---|---|---|---|---|---|
| **C1** Audit | This document, ADR-0021 | `check-wiki.py` | — | — | Merged |
| **C2** Architecture and protocol | Protocol spec section here: message kinds per content type (world operations, document updates, files, presence, chat, checkpoints), versioning rule, batching; the wire gains room for document and chat messages without breaking the format byte; the rename to Collaborate in names users see (old link and `--team-*` flags still accepted) | Wire round-trip tests for each new message; old invite links still parse; refusal text for an older wire version | Size limits for each new message kind from the start | No change to idle cost (measured with the existing idle-session test) | Spec reviewed, wire version bumped, rename merged |
| **C3** Tailscale connectivity | `collab::tailscale` (find, status, sign-in URL, reachability check, whois), invites name the MagicDNS host, host binds where the C3 checks say | Parse tests from recorded `status --json` of each state and platform; a fake `tailscale` script in tests for the commands; reachability check against a closed and an open port | Tailscale identity is display-only (test: a tailnet peer with no membership is refused at the handshake) | `status` runs off the UI thread, at most every 2 s while the wizard is open | Paul's PC hosts and the Mac joins over Tailscale from another network |
| **C4** Five-minute setup | Collaborate menu, first-run wizard (explain, check, install, sign in, connect test, ready), Start Session dialog (name, role, lifetime), Join wizard, `axcenity://` handler and single-instance hand-off, contextual help, the setup guide in `documentation/learn/` with pictures | Wizard state machine tests (each Tailscale state leads to the right step, coming back from the browser resumes); snapshot images of each step; link hand-off test | Links never auto-join without a confirmation showing who and where | Wizard adds nothing to the editor frame when closed | A person new to Tailscale does first setup in five minutes and a later session in under one, timed on Paul's machines |
| **C5** Initial sync and recovery | Recovery checkpoints (host at session start; member's mirror before re-sending offline work), "Compare my copy with the session" for refused offline work, the stop-after-write double-apply fix | Fault tests: host killed between journal write and broadcast; member offline across a host restart; incompatible versions | Recovery files stay outside the project folder | Join time of S10k and Paul's projects recorded | Every fault test passes; nothing is lost in the two-machine cut test over Tailscale |
| **C6** Real-time scene collaboration | Already built (TC2–TC6); this phase re-runs the convergence and editor tests over Tailscale, fixes what that finds, and adds the field-being-dragged bar left from TC4 | Existing suites; new drag-indicator test | — | 3 members dragging on S10k: added frame cost measured | Two machines over Tailscale end identical after a 15-minute scripted session |
| **C7** Live scripts | `collab::doc` (yrs documents on the host and clients), document messages, script and stylesheet tabs bound to documents, remote cursors, own-edit undo, file write-back, outside edits as diffs | Two and three clients typing at once converge; offline typing merges; undo touches only your text; cursor stays put under remote inserts; write-back produces the same file everywhere; property test with random edits and drops | Updates checked for size and capability; a malformed update is refused, not applied | Typing latency on a 5,000-line script; memory per document | Two people type in one script at once on two machines without losing a character |
| **C8** Presence, selections, chat | Chat panel (session-wide, with names and colours, last 200 messages kept by the host in memory, not in the project), mentions of Instances as links, cursor presence from C7 shown in tabs | Chat ordering, rate limit, late joiner gets recent history; presence of a member in a script tab | Chat text length and rate limits; nothing rendered as markup | Chat never blocks the frame | Members can talk without leaving the editor |
| **C9** Version history and recovery | Host-side checkpoints (session start, hourly, Owner on demand, before a restore) in the host's data folder with the journal's attribution, a session History list members can browse, Owner restores for everyone; mirror checkpoints | Checkpoint, list, restore round trip; restore while members are connected | Only the Owner restores; restoring is one transaction everyone sees | Checkpoints are copy-on-write by file hash, so an hour of edits costs only the changed files | A bad session can be rolled back for everyone |
| **C10** Permissions and hardening | `IdentityProvider` seam, per-capability review, rate limits per message kind, invite codes with rate-limited redemption, an audit log the Owner can read, a security review of every handler | Fuzz the wire decoder; every handler with a Viewer and a removed member; replayed handshake | The phase *is* security | Limits measured to never trip in normal editing | Security review recorded here with each finding fixed or accepted |
| **C11** Performance | zstd for big messages, snapshot streaming in pieces for big worlds, presence and chat coalescing, measured on S10k and Paul's projects with 3–4 members | Benchmarks recorded in `docs/benchmarks.md` and the wiki | — | The phase *is* performance | Idle session adds under 0.1 ms; join of S10k under 10 s over Tailscale |
| **C12** Django account integration (document only) | The integration document: providers, token checks, account linking of existing members, migration of ownership, roles and attribution, the versioned API calls the website would offer | Documentation checks only | Threat model for tokens | — | Document merged; no service built |

## 9. Risks

- **Tailscale is a third-party service.** If Tailscale is down or a user can't use it (blocked at work), LAN and a forwarded port keep working; nothing in sync depends on Tailscale.
- **Cross-tailnet sharing has a manual step** in Tailscale's admin console. The wizard can only open the page and explain it; automating it would need a Tailscale API key from the user, which is worse for beginners.
- **CRDT and the host's validation.** A CRDT update can't be half-refused. The host refuses whole updates (size, capability, decode) and never edits them, so all copies stay in step.
- **Rename churn.** Renaming every internal identifier would touch thousands of lines across open work. Only user-facing text, docs, flags and the link change now; internal names follow when those files are next edited.

## 10. Sources

- Tailscale, [Share your machines with other users](https://tailscale.com/docs/features/sharing), [Inviting users vs sharing a device](https://tailscale.com/docs/reference/inviting-vs-sharing), [Share a private game server with friends](https://tailscale.com/docs/use-cases/personal-or-at-home-use/share-private-game-server), [Tailscale Serve](https://tailscale.com/docs/features/tailscale-serve), [`tailscale up`](https://tailscale.com/docs/reference/tailscale-cli/up), [Windows firewall](https://tailscale.com/blog/windows-firewall) (all read 2026-11-12).
- `yrs` 0.28.1 on crates.io: [y-crdt/y-crdt](https://github.com/y-crdt/y-crdt) (MIT); dependencies checked against `Cargo.lock` at `d45d70b`.
