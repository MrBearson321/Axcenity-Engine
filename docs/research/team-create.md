# Team Create: architecture and sync-model proposal

Section 8 of Paul's brief "Editor Usability, Core Features, Documentation & Collaboration Improvements" (2026-11-11), Phase 7. The brief asks for the architecture and synchronization model **before** any implementation. This is that proposal. **Approved 2026-11-11:** Paul accepted the recommended plan with the defaults D1–D5 (decision card). TC1 is under way.

- **Status:** Approved (2026-11-11), with the recommended answer to every decision in [§14](#14-decisions-for-paul). The matching ADR is [ADR-0020](../../wiki/Architecture/Architecture-Decisions/ADR-0020-Team-Create-Session-Authority-And-Operations.md) (Accepted).
- **Date:** 2026-11-11
- **Engine:** `main` at `1bdb77a` (PR #132)
- **Relies on:** [ADR-0002](../../wiki/Architecture/Architecture-Decisions/ADR-0002-Instance-Tree-In-A-Generational-Arena.md) (arena IDs), [ADR-0010](../../wiki/Architecture/Architecture-Decisions/ADR-0010-Listen-Server-Replicas-And-Network-IDs.md) (replicas with shared IDs), [ADR-0016](../../wiki/Architecture/Architecture-Decisions/ADR-0016-Text-Project-Format-With-Migrations.md) (text project files), [ADR-0017](../../wiki/Architecture/Architecture-Decisions/ADR-0017-Editor-Play-In-Process-On-A-Cloned-World.md) (Play on a clone), [ADR-0008](../../wiki/Architecture/Architecture-Decisions/ADR-0008-Desktop-Texture-Cooking.md) (per-machine cooked cache), the [development policy](../development-policy.md) (no infrastructure before it's needed; internet hosting and M11 deferred), and the MCP toolkit's change log and undo rules ([editor/mcp.md](../editor/mcp.md)).

## 1. Recommendation in one page

- **One authority per session, in a hybrid deployment.** A Team Create session has exactly one *session host* that holds the authoritative World and orders every edit. The same host code runs in three places: inside the owner's editor ("Host from this editor", like the listen server of ADR-0010), as a small headless binary anyone can self-host (`axcenity-collab`, on a spare PC, a NAS or a cheap VPS), and later, if Axcenity ever runs a service, on Axcenity's own machines. Nothing in the editor depends on which one it talks to. This costs nothing to run today and keeps the door open for a hosted service.
- **Operation-based sync, ordered by the host.** Each undo step a user makes becomes one *transaction* of small typed operations (create subtree, destroy, reparent, set properties, set attribute, tag, terrain chunks, file version). The client applies it at once (no lag), sends it to the host, and the host validates it, gives it the next sequence number and sends it to everyone. Properties are last-writer-wins in the host's order; the tree is checked by the host (no cycles, no writes into deleted Instances). This is the model Figma and Unreal's Multi-User Editing use, and it fits the engine because every editor edit already goes through `History` as a before/after record.
- **Never a whole-project overwrite.** The host's project folder stays a normal Axcenity project (ADR-0016 text files, so git keeps working). The host writes it on autosave and keeps an append-only journal of transactions between saves, so a crash loses nothing.
- **Identity without a server, accounts later.** There is no Axcenity account system today, and building one means running a service. For now each person gets a *Team Create identity*: a display name plus a device key pair made on first sign-in, kept on their machine. The owner invites people with a signed, expiring invite link; the host binds the invitee's public key to a role. When an Axcenity account service exists, it becomes a second identity provider behind the same interface. Local editing never needs any of this.
- **Roles as capability sets.** Owner, Editor and Viewer map to sets of capabilities the host checks on every operation, so Builder, Scripter or Commenter roles can be added later as data.
- **Scripts and other documents by file, with editing locks.** In v1 a script, block script, graph or other document tab is edited by one person at a time ("Alex is editing"); saves are whole-file versions. Live co-editing of text is a later phase.
- **Reuse, don't duplicate.** `History` steps are the op source, the world's change log is the safety net, `net`'s handshake checks (protocol, class defaults fingerprint) and its loopback network emulation are reused, MCP's "undo only your own steps" rule and "by you / by others" change summaries carry over, and Version History checkpoints protect against bad sessions.

## 2. What exists today

| Piece | Where | Use for Team Create |
|---|---|---|
| Server-authoritative replicas whose Instances keep the server's IDs; a replica numbers its own Instances from 2³¹ | `core/src/arena.rs` (`make_replica`, `insert_at`, `LOCAL_BASE`), `World::empty_replica`, `World::create_at`; ADR-0010 | Joining clients hold a replica of the host's edit World; IDs are the network IDs. Needs one extension: several writers creating Instances ([§7.4](#74-ids)). |
| Snapshot and diff replication (made Instances with non-default properties only, names, parents, properties, attributes, tags, destroyed) over postcard | `net/src/replicate.rs`, `wire.rs` | The join snapshot and value encoding are reused. The per-frame `Replicator` itself is not: it hides server-only services, sends motion separately and carries no authorship or acknowledgements. |
| Join handshake with readable refusals: protocol version, class-defaults fingerprint, game version, password | `net/src/handshake.rs` (`admit`, `class_defaults`) | Same checks and the same refusal style for collaborators. |
| Loopback transport with emulated latency, jitter and loss | `net/src/transport.rs` (`Conditions`) | The same idea for multi-client collaboration tests. |
| Every editor edit is one labelled undo step with exact before and after values | `editor/src/history.rs` (`StepKind`: `Fields`, `Many`, `Tree`, `Files`, `Blobs`, `Terrain`, `Strings`, `Annotations`, `World`, `Edits`); route: `EditorApp::change` | The op source: a step becomes one transaction. |
| World change log (`World::record_changes`, `take_changes`), numbered changes | `core/src/world.rs` | Debug/test check that nothing changed the edit World outside a recorded step (an unsynced edit). |
| MCP: copy-run-diff writes as one step, undo limited to the client's own steps, "what changed since mark n" grouped by you and others | `editor/src/app/mcp/ops.rs`, `app/mcp/changes.rs` | Same undo rule; the same summary feeds the Team Create activity list. AI clients connected to one collaborator's editor simply act as that collaborator. |
| Version History, checkpoints, Back Up Project | `editor/src/app/version_history.rs` (Phase 72) | A checkpoint when a session starts and periodically; recovery copies of unsent work. |
| Editor Play on a cloned World | ADR-0017 | Each collaborator plays locally on a clone; remote edits keep arriving in their edit World underneath. No change needed. |
| Dependencies already in the tree | `tokio` (rt), `rustls` (via `ureq`), `tungstenite` (indirect), `ring`, `sha2`, `postcard` | TLS, WebSocket framing, Ed25519 signatures and content hashes need no new crate families. |
| Accounts | none: `Player.UserId` is a random per-install number (`docs/architecture/networking.md`) | Nothing to integrate with yet ([§5](#5-accounts-and-identity)). |

The engine comparison and gap analysis list live collaboration as missing (row "Live collaboration", category F, Priority 4: "needs a server and accounts"). This proposal removes the server requirement for a first version and defers only the hosted parts.

## 3. Goals and non-goals for v1

**Goals:** several people edit the same project at once over LAN or the internet; edits appear for others within one round trip; no edit is silently lost; every request is authenticated and checked against the member's role on the host; disconnects and reconnects are routine; local-only editing is unchanged and never asks anyone to sign in.

**Not in v1:** an Axcenity-run service or account system (Phase TC8, with M11); live co-editing inside one script (TC5 keeps per-file locks); merging work done offline on a collaborative project back in automatically (it is kept as a recovery copy instead, [§9](#9-reliability)); several collaborators on different levels of one project at the same time; comments and review tools; mobile or web.

## 4. Where the authority lives

### 4.1 Options

| Option | For | Against |
|---|---|---|
| **Hosted backend** run by Axcenity (accounts, project storage, collaboration servers, relay) | Always on; no setup for users; easy invites; how Roblox Team Create works | Needs servers, operations, uptime, backups, abuse handling, privacy policy and data protection, and recurring cost; the development policy defers hosting; one server implementation becomes a dependency of every collaborator |
| **Self-hosted only** (a server binary users run) | No cost to Axcenity; data stays with the team | Every team needs someone to run a server, even two people on a LAN |
| **Peer-to-peer** (no authority; CRDT merge on every peer) | No server at all | The Instance tree with reparenting, references and unique services is hard to make convergent without an authority (move cycles, deleted parents); every peer must enforce permissions, so a modified client can't be stopped; NAT traversal needed anyway |
| **Hybrid: one session authority that can run in the owner's editor, as a self-hosted binary, or later as a hosted service** | Zero infrastructure now; same protocol everywhere; permissions enforced in one place; a hosted service can be added without changing the editor | The owner's editor hosting means the session ends when they leave (the self-hosted binary covers always-on); internet reachability is the team's job until a relay exists |

**Recommended: hybrid.** It satisfies the brief's "scale without forcing the entire engine to depend on a single collaboration server implementation" and the development policy's "no infrastructure before it's needed".

### 4.2 Layout

```
crates/collab          protocol, operations, host state machine, client state machine,
                       identity and invites, transport trait (no egui, no GPU, no scripts)
crates/collab-server   axcenity-collab: headless host binary (project crate + collab only)
crates/editor          app/team_create.rs (client and in-editor host), team_create/ (panel, presence)
```

`collab` depends on `core`, `project` and `net`'s wire encoding only, so the headless host keeps ADR-0001's rule (no GPU, audio or window on a server). The editor is a client of `collab`; when hosting from the editor it also runs the host state machine on its own edit World, the way a listen server works.

### 4.3 Transport

**WebSocket over TLS 1.3 on TCP** (`tungstenite` with `rustls`, one thread per connection; TC3 found no need for an async runtime), carrying postcard messages after a format byte, like the game wire. Edits are small, reliable and ordered, so TCP's ordering is what we want; WebSocket over TLS goes through home routers, corporate proxies and tunnels (Cloudflare Tunnel, Tailscale Funnel) that UDP doesn't. The game transport (`renet` over UDP) isn't reused: it runs unsecured until M11 and its reliable channel caps messages at 5 MiB. QUIC (`quinn`) was considered; it adds a new dependency family for no gain at editing rates.

Reachability in v1: same LAN (shown in the Team Create panel by LAN discovery, like games), a forwarded port, or an overlay network the team already uses (Tailscale, ZeroTier). A relay for NAT traversal comes with the hosted service (TC8).

## 5. Accounts and identity

The brief asks that users sign into their Axcenity accounts. There is no Axcenity account system, and creating one means running an always-on service that stores personal data. Options:

| Option | For | Against |
|---|---|---|
| **Build Axcenity accounts now** (account service, sign-up, email verification, password reset or a third-party login behind it) | Matches the brief word for word; one identity across machines | A hosted service with personal data before anything needs it; cost; security and privacy obligations; blocks every other phase |
| **GitHub sign-in as the identity** | No passwords to store | Ties Axcenity to GitHub; the host would have to see a GitHub token or call GitHub's API for every login; not every user has GitHub |
| **Team Create identity: device key pair + invites now; Axcenity accounts later as a second provider** | No service; strong cryptographic authentication; invites and roles work the same later | Your identity is per machine until accounts exist (a second machine is added by inviting it, or by exporting the identity) |

**Recommended: the third.** Concretely:

- **Sign in** (Settings > Team Create, or the first time you share or join) asks for a display name and a colour and makes an Ed25519 key pair (`ring`). The private key stays in the per-user data folder (owner-only file permissions; the OS keychain is a later improvement). **Sign out** forgets it on this machine. The identity's ID is the public key's fingerprint, shown short ("A3F9-…").
- **Invites**: the owner picks a role and an expiry (default 7 days, single use) and gets a link `axcenity://teamcreate/join?host=…&cert=…&invite=…`, also shown as text to paste. It names the host address, the fingerprint of the host's TLS certificate (so the client trusts exactly that host, no certificate authority needed) and a one-time secret. Redeeming it binds the joiner's public key to the role.
- **Authentication**: after TLS, the host sends a random challenge; the client signs the challenge together with the host's certificate fingerprint (binding the signature to this connection). The host looks the key up in the project's member list. Every later message rides on that authenticated connection; a reconnect signs again.
- **Later (TC8)**: an `IdentityProvider` interface with two implementations, device keys and Axcenity accounts (OpenID Connect sign-in with short-lived tokens the host verifies against the service's published keys). Members can then be invited by account name, and an account links its devices.

## 6. Projects, membership and permissions

- **Local vs collaborative.** A project is local until its owner chooses **File > Share with Team Create…**. A collaborative project shows a badge on the start screen, in the title bar and in the Team Create panel. Collaborators get a *mirror*: a normal project folder in their data folder (`TeamCreate/<project id>/`) that the session keeps in step; they can open it offline read-only, or copy it out ("Save a local copy") as an ordinary local project.
- **Where membership lives.** On the host, outside the project folder (`<data>/TeamCreate/hosted/<project id>/members.json`), so a project pushed to a public git repository never carries members' names or keys. The project ID is a random UUID made when the project is first shared.
- **Roles are capability sets** the host checks on every operation:

| Capability | Owner | Editor | Viewer |
|---|---|---|---|
| Open the session, see presence, read everything | ✓ | ✓ | ✓ |
| Edit the world (Instances, terrain, string table) | ✓ | ✓ | |
| Edit scripts and documents | ✓ | ✓ | |
| Add, change and delete asset files | ✓ | ✓ | |
| Save a checkpoint | ✓ | ✓ | |
| Invite and revoke invites | ✓ | optional (owner's setting) | |
| Change roles, remove members | ✓ | | |
| Project settings, levels, Build Game settings | ✓ | | |
| Plugins (`plugins/`) | ✓, and each member approves locally | | |
| End the session, transfer ownership | ✓ | | |

Adding a role later (Builder: world only; Scripter: scripts only; Commenter) is a new row of data, not code. Per-subtree permissions (a locked Folder) fit the same check later.

A Viewer receives the whole project, scripts included, so "viewer" means *can't change it*, not *can't copy it*. The panel says so when inviting a Viewer.

## 7. Sync model

### 7.1 Why host-ordered operations

| Model | Verdict |
|---|---|
| Whole-file sync (send `world.json` on save) | Rejected by the brief; also loses concurrent work and is slow (S10k's `world.json` is 10 MB and ~5.5 s to load) |
| State diffing (each client diffs its World every frame, like the game `Replicator`) | Works for one writer; with many writers it can't tell who changed what or which change came first |
| CRDT on every peer (Automerge, Yjs) | Convergent without an authority, but tree moves need special algorithms, IDs and references don't fit the arena, and permissions still need a trusted place |
| **Host-ordered operations with optimistic local apply** | Simple to reason about (one order), permissions and validation in one place, matches `History`'s before/after records, used by Figma and Unreal Multi-User |

### 7.2 Operations

A **transaction** is one undo step: `{ client_seq, base_seq, label, ops }`. Operations:

| Operation | Contents | Merge rule |
|---|---|---|
| `CreateTree` | Instances (ID, class, parent, index, name, non-default properties, attributes, tags), parents first | IDs from the creator's block ([§7.4](#74-ids)); refused if the parent is gone (see [§8](#8-conflict-handling)) |
| `Destroy` | top Instance; its subtree goes too | Delete wins; the host keeps a tombstone for Restore |
| `Reparent` | Instance, new parent, index | Refused if it would make a cycle or the parent is gone |
| `SetProps` | Instance, `[(property, value)]` (Name included) | Last writer in host order wins, per property |
| `SetAttribute`, `Tag` | Instance, name, value or added/removed | Per attribute / per tag, last writer wins |
| `TerrainChunks` | Terrain, `[(chunk, voxels)]` from a brush stroke (`TerrainEdit`) or a whole landscape | Per chunk, last writer wins |
| `StringCell` | string table key, language, text | Per cell, last writer wins |
| `FileVersion` | path, base hash, new hash or none | Compare-and-swap on the base hash ([§7.7](#77-scripts-documents-and-asset-files)) |

Values use `net`'s wire encoding (postcard, `Value` as on the game wire). A whole-world step (prefab Apply/Revert, `History::record_world`) is turned into the operations above by comparing the before and after Worlds with the replication comparison code, so no "replace the world" operation exists.

### 7.3 Where operations come from

The editor already funnels every edit through `EditorApp::change` and the `History::record_*` functions, and each `StepKind` holds exact before and after values. A new `collab::from_step` turns a step into a transaction when it's recorded or extended (a drag merges into its open step; the client sends the dragged field at most 20 times a second and once more when the gesture ends, so others watch it move). Undo and redo produce transactions the same way.

Safety net: in a session, the edit World's change log stays on (as the MCP host already does). After each frame, any change the log names that no step covered is an *unsynced edit*: a test failure in debug builds, a logged warning plus a correction from the host in release. This is how we catch a tool that bypasses `History`.

### 7.4 IDs

ADR-0010 makes `InstanceId` the network ID, and replicas number their own Instances from 2³¹. Team Create keeps that and adds **ID blocks**: the host hands each writer (itself included) blocks of free arena indices, 256 at a time, taken in order so the arena stays dense, and a client asks for more when half its block is used. A client creates Instances only inside its blocks (`Arena::insert_at`), so concurrent creates never clash and no ID ever has to be rewritten. This needs a small `core::Arena` addition (allocate from given ranges instead of the free list in a collaborative World) and is the first thing TC1 proves; the fallback, if it turns out awkward, is provisional IDs from the local range that the host maps on acknowledgement.

IDs live only as long as the session's World. A session always starts from the host's saved project, which stores references as paths (ADR-0016), so no persistent IDs are needed.

### 7.5 One edit's round trip

1. The user edits; the editor applies it locally at once and records the step.
2. The client sends the transaction and remembers it as *pending*, with the fields it touched.
3. The host checks the member's capabilities and validates every operation (class exists, property exists and takes that type, sizes within limits, tree rules). All of a transaction applies or none of it does.
4. Accepted: the host applies it, gives it the next sequence number, appends it to the journal and sends it to everyone (to the author it doubles as the acknowledgement). Refused: the author gets a refusal with the reason and the authoritative current state of the Instances it touched, and the editor puts those back and says why in a toast.
5. Other clients apply incoming transactions in sequence order. A field that has a pending local value keeps showing the local value until the author's own transaction comes back (Figma's rule): since the host orders that later, the end state is the same everywhere.

The selection, open Explorer branches, camera and panels are per user and never synced; an incoming `Destroy` drops the Instance from the local selection.

### 7.6 Joining

Joining sends: the handshake ([§9](#9-reliability)), then a postcard snapshot of the host's edit World at sequence *n* (all services, server-only ones included: collaborators edit ServerScriptService too), then the file manifest (path and SHA-256 of every synced file), then live transactions from *n + 1*. The client downloads only files whose hash it doesn't already have in its mirror. S10k's game snapshot is about 1 MB of postcard (M7), so the world part of a join is small; first-time asset download is the slow part and shows progress.

### 7.7 Scripts, documents and asset files

- **Synced**: everything a project folder holds except per-machine and per-user data. **Not synced**: `.axcenity/` (cooked cache, ADR-0008, which each machine rebuilds; local version history; editor layout), and `plugins/` except by the owner with each member's local approval, since plugins run Lua with the editor's powers.
- **Files are content-addressed.** A new or changed file is uploaded once by its SHA-256, then a `FileVersion { path, base, new }` operation publishes it. The host accepts it only if `base` matches the current version, so two saves of the same file can't overwrite each other silently. Files travel in 1 MiB pieces (uploads first, then the transaction; a reconnecting client uploads its pending ones again). The host keeps uploads under its own `.axcenity/teamcreate/files/` by hash and reads files that were there at the start from the project folder, checking the hash; it never writes the project folder itself, which the owner's editor keeps like every member keeps their own copy.
- **Mirrors.** Each member's editor works on a folder (`collab::mirror`). Per file it remembers the version the folder and the session last agreed on, so each sync tells a local save (published), a remote one (fetched and written) and both (the local one becomes the conflict copy) apart; a refused save puts that version back, so it turns into a conflict copy too. What it remembers survives in `.axcenity/teamcreate/mirror.json`, so edits made away from the session are published on rejoining. A file someone else has locked waits.
- **Editing locks for documents.** Opening a script, block script, UI graph, material, data table, dialogue or behavior tree tab with an unsaved change takes a soft lock the host grants to one member at a time; others see "Alex is editing" and the tab read-only until it is saved or closed (the lock also ends when Alex disconnects for longer than a minute). If a save still loses the compare-and-swap (after a reconnect, say), nothing is overwritten: the editor writes the user's text as `<name>.conflict-<user>.lua` beside it and shows both.
- **Imports** on a client upload the imported files, then send `FileVersion`s and the `CreateTree` of the placed model as one transaction, so one undo step still undoes an import for everyone.
- **MCP writes** in a collaborator's editor go through the same steps, labelled `AI (<client>): …`, and sync like any of that member's edits.

### 7.8 Undo and redo

Each member undoes and redoes **only their own steps**, as MCP clients already do. Undo sends the step's inverse as a new transaction. Per field, if someone else has changed it since (its current value isn't the step's "after"), that field is left alone and the toast says so ("Some changes weren't undone: Alex changed Color since"). Undoing a create whose Instances others have since edited asks first. The history is per member and per session.

### 7.9 Presence

Presence messages are separate from transactions and not journaled: each member's selection, camera position and direction, active tool, and the field or document they are editing, sent at most 10 times a second and only when it changes. The editor draws other members' selections as outlines in their colour with a name tag, marks Explorer rows they have selected with their colour, and shows a coloured bar on a Properties field someone is dragging. A member's camera can be followed from the Team Create panel.

### 7.10 Saving on the host

The host's project folder is the project. The host writes it in the normal format (ADR-0016) after changes settle (every 30 seconds with changes, and when the last member leaves) and appends every accepted transaction to `.axcenity/teamcreate/journal` between saves (flushed per transaction). After a crash the host loads the last save and replays the journal. Version History makes a checkpoint when a session starts and every hour of editing, so a bad session can be rolled back. A collaborator's **File > Save** asks the host to save and checkpoint; it never writes the host's folder directly.

Levels: in v1 a session edits one level at a time (the host's open level); the owner switches levels for everyone, with the usual unsaved-changes check.

## 8. Conflict handling

| Situation | Rule | What users see |
|---|---|---|
| Two members change the same property | Host order; the later one wins everywhere | Nothing special; presence shows who else is on that field |
| Two members change different properties of one Instance | Both apply (per-property merge) | Nothing special |
| Edit of an Instance another member deleted | Delete wins; the edit is refused | Toast "Alex deleted Door" with **Restore** (the host's tombstone re-creates it at its old IDs) |
| Create inside a parent another member deleted | The created subtree goes to the deleted parent's nearest surviving ancestor (usually Workspace) | Toast saying where it went; nothing is lost |
| Two reparents that would make a cycle | The second is refused with a correction | Toast "Couldn't move: Alex moved it" |
| Reference property to a deleted Instance | Allowed; it goes stale as references already can (`Value::Instance` lookups are `Option`) | Shown as a missing reference in Properties |
| Two saves of the same script or file | Compare-and-swap; the loser's text is kept as a `.conflict-<user>` copy | Both versions in the Assets panel |
| Terrain strokes on the same chunk | Per chunk, host order | Nothing special |
| Undo of a field someone changed since | That field is skipped | Toast naming who changed it |
| Duplicate or out-of-order transactions | Host drops a `client_seq` it already applied; clients apply strictly by sequence and resync on a gap | Nothing |
| A member's role is lowered or they are removed mid-session | Takes effect on their next transaction; their pending transactions are refused | Read-only banner, or the session closes with a reason |
| Duplicate (Ctrl+D, Duplicate in Explorer) | A normal `CreateTree` where the copy is placed; collision-avoiding placement doesn't apply, as the brief asks for duplicates | Nothing special |

## 9. Reliability

- **Handshake and version compatibility.** The client's first message carries the Team Create protocol version, the engine version, `handshake::class_defaults()` and the project's `format_version`. In v1 all must match the host's; a mismatch is refused with a sentence each side understands ("The host runs Axcenity 0.2.0-alpha.2 and you run 0.2.0-alpha.1: update in Settings > Updates."). Mixed versions can come later with operation versioning.
- **Disconnects.** The client keeps pending transactions and buffers new ones (editing continues; the status chip says "Offline, 3 changes waiting"). On reconnect it signs in again and sends the last sequence it saw; the host replays what it missed from the journal, or sends a fresh snapshot if that is older than the journal keeps. Pending transactions are re-sent; the host drops any `client_seq` it already applied, so nothing applies twice.
- **Host restarted or session ended while a client was away.** The client first writes its pending transactions and their after-values as a recovery file and a local Version History checkpoint of its mirror, then rejoins and re-sends them; any that no longer apply (their Instances are gone) are listed in a Recovered Changes window, not dropped silently.
- **Host in the owner's editor quits.** Members get "The host left the session", their mirror stays open read-only, and unsent work goes to the recovery file. The self-hosted binary avoids this for teams that need an always-on project.
- **Expired or revoked access.** Device-key sessions don't expire by time; removal or a role change takes effect at once on the host. With accounts (TC8), token refresh happens in the background and a failed refresh puts the editor in read-only with a Sign in again button, keeping local work.
- **Integrity.** All of a transaction applies or none does, on the host and on clients. The journal is written before the transaction is broadcast. The host never accepts a write outside the project folder (paths are checked as `Assets` already does).

## 10. Security

- **Every request authenticated**: TLS 1.3 to a pinned host certificate, then a signed challenge bound to that connection; no unauthenticated message is processed except the handshake.
- **Access checked on the host**: membership and capabilities per operation; the client's UI hides what a role can't do but is never trusted.
- **Validation**: known classes and properties with the right value types (the World's typed `set_property` errors), size limits per transaction (1 MB) and per file (configurable, default 256 MB), rate limits per member, paths inside the project only, an allow-list of file extensions.
- **No code runs on the host from collaborators.** The edit World doesn't run scripts; Play happens on each member's own machine, in the Luau sandbox. Plugins are owner-only and need each member's local approval, as above.
- **Invites**: single-use by default, expiring, revocable; they carry no long-lived secret.
- **Public repository**: no keys or secrets in the repo; host certificates and keys are made at first run and stored in the host's data folder; members and keys live outside the project folder.
- **Logs**: the host keeps an audit log of who changed what (transactions with member, time and label) for the session's journal lifetime.

## 11. User experience

- **Team Create panel** (a dock panel like Explorer, fits the 260-point column with `responsive.rs`): the project (owner, local or collaborative badge), connection (host, round trip, status: Connected, Syncing n, Reconnecting, Offline with n waiting), collaborators (colour, name, role, online, idle or offline, what they're editing; click to follow), invitations (Create invite link, pending invites, revoke), permissions (owner changes roles or removes members), and **Leave session** (members) or **End session** (owner). An activity list below reuses MCP's change summary ("Alex added 3 Parts in Workspace").
- **Title bar chip** next to the update chip: up to four member avatars and a sync dot; click opens the panel. Nothing else takes screen space.
- **Viewport, Explorer and Properties** indicators as in [§7.9](#79-presence).
- **Start screen**: **Join Team Create…** (paste an invite), collaborative projects listed with a badge, and open-able offline as read-only.
- **File > Share with Team Create…** walks the owner through signing in (if needed), choosing "host from this editor" or "use a server", and the first invite.
- **Settings > Team Create**: identity (name, colour, fingerprint), sign in and out, default invite role and expiry.

## 12. Testing

| Level | What | Where |
|---|---|---|
| Operations | Each operation applies and inverts exactly; validation refuses each bad input with its reason | `collab` unit tests, cloud |
| Conflicts | Every row of [§8](#8-conflict-handling) as a test with two or three clients | `collab`, in-memory transport |
| Convergence | Property-based: 3 clients make random edits with random latency, reordering of delivery between clients, loss and reconnects; afterwards every World equals the host's and no accepted transaction is missing | `collab`, cloud; seeds kept for failures |
| Editor | Two or three headless `EditorApp`s in one test: an edit, a drag, undo, delete-while-editing, an import, a script lock; no unsynced edits (change log check) | `editor` tests, cloud |
| Faults | Kill the host mid-transaction and replay the journal; cut a client for 5 s and 5 min; stale invite; revoked member; version mismatch | `collab` and `editor` tests |
| UI | Panel and indicators drawn at 260 points wide and wide; `AXCENITY_SNAPSHOTS` images | cloud |
| Performance | Editor S10k frame with an idle session and with 3 members dragging: target under 0.1 ms added when idle; join time for S10k and Paul's real projects | `--bench`; Paul's PC |
| Real network | Paul's PC hosting, the laptop joining over LAN and over a tunnel; pulled cable; sleep and wake | Paul's PC and laptop |

## 13. Phased plan

Each phase is one or more PRs against `main`, tested locally as the policy asks.

| Phase | Builds | Done when |
|---|---|---|
| **TC0** | This proposal and ADR-0020 | Done: approved 2026-11-11 |
| **TC1** Core | `collab` crate: operations, transactions, host and client state machines, ID blocks (`Arena` range allocation), in-memory transport | Done: `crates/collab`; a test per conflict rule and a convergence test (150 seeds by default, 3,000 checked) pass. Terrain chunks, string cells and file versions are left for TC2 and TC5 |
| **TC2** Editor over loopback | `History` step to transaction, remote transactions into the edit World, own-step undo, unsynced-edit check, in-editor host | Done: instead of converting `History` steps, the World keeps an edit journal (`World::record_edits`) and `collab::capture` turns whatever changed it into one transaction per frame, so every tool, plugin and AI client syncs with no code of its own and nothing can bypass it; writers' ID blocks live in the arena. The editor side is `editor/src/app/team.rs` (held while a gesture is open; undo leaves alone what others changed since, with a warning; SplineMesh pieces stay local via `World::unrecorded`). Tests: capture tests, the convergence test with direct edits (3,000 seeds) and two headless editors in one session. Left for later: undoing a whole-world step (Console, prefab Apply) is refused in a session; terrain, the string table and files aren't shared until TC5; a shared Instance destroyed outright (rather than deleted) makes that copy reload |
| **TC3** Network and identity | WebSocket over TLS, device keys, sign in and out, invites, roles enforced, handshake refusals | Built (LAN test on Paul's PC and laptop pending): `collab` gained `wire` (format byte plus postcard; every simulated-network test message goes through it), `identity` (Ed25519 device keys), `access` (members by key, single-use 7-day invite links, kept in the editor's data folder, never the project), `handshake` (JSON hello, challenge, proof, admitted or refused with a sentence for the joiner), `socket` (WebSocket over TLS 1.3 to a pinned self-signed certificate, one thread per connection, a connection quiet for 15 s counts as lost) and `session` (`HostSession` on its own thread, `Remote` reconnecting with backoff). The editor hosts with `--team-host` (port 47810; it joins its own session over loopback and prints an invite) and joins with `--team-join <link>`. Tests over real sockets: joining, edits from three members with drops, a cut network, roles, removal, used, expired, unknown and revoked invites, another key, another version or build, another certificate, and two headless editors in one session. Changes from the plan: the transport is plain threads with the synchronous `tungstenite`, not `tokio-tungstenite` (a handful of connections needs no async runtime); the hello doesn't carry the project's `format_version`, which follows from the engine version. Left for TC4: Sign in and Sign out, invites and members in the UI. Left for TC6: size and rate limits per member (a message may be up to 256 MiB to the host) |
| **TC4** Interface | Team Create panel, title bar chip, presence in viewport, Explorer and Properties, Share and Join flows | Built (Paul tries it): the roster and presence (`collab::presence`: name, colour, role, online, selection, camera; at most 10 a second, kept by the host for late joiners, never logged); the Team Create panel (`editor/src/app/team_panel.rs`: Share, Join by pasting a link, connection status, collaborators with roles the owner changes or removes, invite links for Editors or Viewers, revoke, Leave or End session); File > Share with Team Create… and Join Team Create…; a title bar chip with a sync dot and up to four avatars; others' selections outlined in their colour with a name tag in the viewport, colour bars on their Explorer rows and "Also selected by" in Properties. Left: following a member's camera, the field-being-dragged bar and Settings > Team Create (name, colour, sign out) come with TC6; badges with TC5's mirrors (the start screen's Join came with TC5). UI snapshots need a GPU, so they are still to take on Paul's PC |
| **TC5** Files | Content-addressed file sync, mirrors, document locks, conflict copies, imports as one transaction | Built (Paul tries it): sync core (`collab/src/files.rs`, `mirror.rs`, `files_tests.rs`, a real-socket test: saves by hash with compare-and-swap, uploads and fetches in pieces, locks with a one-minute grace after a drop, files and World edits as one transaction, mirror folders with conflict copies) and the editor (`editor/src/app/team.rs`: the host serves its project folder; a joiner fetches the files into its own folder under the Team Create data folder and opens it as the project with the session's World; every document tab someone else is editing (scripts, stylesheets, block scripts, UI Event Graphs, behavior trees, dialogues, data tables, materials) is read-only with a lock line saying who; what a teammate saves is read again, `strings.json`, `input.json` and UI graphs included; an import is one step for everyone; the start screen's Join… (and `--team-join <link>` with no folder) joins with no project open, and closing or opening another project leaves the session). Left: the two-machine test with scripts and imports; string table edits are shared when saved, and two members changing it before either saves keep the last save |
| **TC6** Reliability | Journal and autosave, reconnect and resync, recovery file, Recovered Changes window | Fault tests pass |
| **TC7** Self-hosted server | `axcenity-collab` binary with persistence and an admin command line (members, invites, sessions) | Runs on Windows and Linux; editor can't tell it from the in-editor host |
| **TC8** Deferred | Axcenity accounts as an identity provider, a hosted service and relay | With M11 / internet hosting, when Paul decides |

## 14. Decisions for Paul

| ID | Question | Recommended | Alternatives |
|---|---|---|---|
| **D1** | Where does the authority run? | **Hybrid**: in the owner's editor and as a self-hostable binary now; an Axcenity-hosted service later, same protocol | Hosted service now; self-hosted binary only |
| **D2** | How do people sign in? | **Team Create identity** (device key, name, colour) with invite links now; Axcenity accounts as a provider in TC8 | Build Axcenity accounts now (needs a hosted service); GitHub sign-in |
| **D3** | Scripts and other documents in v1 | **One editor per document at a time** (soft lock, whole-file versions, conflict copies) | Live co-editing of text now (a text CRDT such as `yrs`, a new dependency and a larger TC5) |
| **D4** | Levels in v1 | **One shared level per session**; the owner switches | Members on different levels at once (more state per member) |
| **D5** | Default role and invite policy | **Editor**, single-use links that expire after 7 days; only the owner invites unless they allow Editors to | Viewer by default; reusable links |

The transport (WebSocket over TLS), the operation model and ID blocks are engineering choices this proposal makes; say if you want any revisited.

## 15. Risks and simplifications

- **Edits that bypass `History`** wouldn't sync. Resolved in TC2: sync reads the World's own edit journal, so there's no way around it.
- **ID blocks** touch the arena, a core type every system uses. TC1 measures that nothing changes for non-collaborative Worlds (S10k bench) before going further; the fallback is provisional IDs with host mapping.
- **Whole-world steps** (prefab Apply, level open) become large transactions; fine for v1, measured in TC2.
- **Owner-hosted sessions** end when the owner leaves; documented, and TC7 is the answer for always-on.
- **Internet reachability** without a relay needs a forwarded port or an overlay network; the Share flow explains the options in plain words.
- **Exact version match** in v1 means a team updates together; the Version Manager's update chip makes that a click.
- **Not chosen**: CRDT for the Instance tree (no authority means no permission enforcement and hard move semantics); reusing the game `Replicator` as is (one writer, hides server-only services); renet for collaboration (unsecured until M11).
