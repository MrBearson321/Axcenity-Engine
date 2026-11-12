# Collaborate: audit of Team Create and the plan for Paul's 2026-11-12 brief

Paul's brief "Axcenity Collaborate – Additional Architectural Decisions and Requirements" (2026-11-12) sets the direction for **Collaborate**, the feature built so far as Team Create (TC1–TC7, [team-create.md](team-create.md), [ADR-0020](../../wiki/Architecture/Architecture-Decisions/ADR-0020-Team-Create-Session-Authority-And-Operations.md)). Its first priority is an audit of what exists and what can be reused. This is that audit, the decisions it leads to ([ADR-0021](../../wiki/Architecture/Architecture-Decisions/ADR-0021-Collaborate-Tailscale-Live-Scripts-And-Identity-Seam.md)) and the phased plan C1–C12, one phase per priority in the brief.

- **Status:** C1 (this audit) done; C2's protocol is [§9](#9-protocol-c2), its rename is PR #197. C3 is built (§5.1) and its two-machine test over Tailscale passed (§5.3). C4's wizard, Start Session dialog, link handling and setup guide are built and were used for a real session (§5.4, §5.6). C5's fixes and checkpoints are built (§5.5). C6's field-being-dragged bar is built and passed with two people over Tailscale (§5.6). C7's live scripts and stylesheets are built and passed a two-machine typing test ([§4.1](#41-built-c7)). C8's chat and others' cursors in script tabs are built (§5.7). C9's host checkpoints and restores are built (§5.8). C10's identity seam, per-message budgets, handshake limits and audit log are built and its security review is [§6.1](#61-security-review-c10). C11's packing of big messages is built and its measurements are [§9.6](#96-performance-c11); C12 follows the plan in [§8](#8-phased-plan-c1c12).
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
| Batching and serialization | One transaction per frame or per gesture (drags at most 20 a second), postcard on the wire behind a format byte (`wire.rs`) | Reuse; big messages packed with zstd since C11 (§9.6) |

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

### 4.1 Built (C7)

- **`collab::live`**: `LiveDoc`, one `yrs` 0.28 `Doc` with one `Text`, its changes as v1 updates and state vectors. `set_text` turns the tab's new text into one changed span (typing, a paste) on character boundaries. `is_live`: `.lua` and `.css` files; everything else keeps its lock. Built with `yrs`'s `sync` feature, so a host with documents can live on the session's thread.
- **Host** (`host.rs`): `DocOpen` opens the document (from the file the first time, with a random **origin** number) and answers `DocState` with what the member lacks, the host's state vector and the origin. `DocUpdate` is checked (connected member, may edit scripts or assets for the path, 1 MiB, decodes) and refused whole with `DocRefused` (the member's copy is then opened again from the host's), or applied and passed to the others. Typing that settles for `WRITE_BACK_AFTER` (2 s) is written to the file as an ordinary `FileVersion` transaction by whoever typed last (client number 0, so it's never mistaken for their own edit), so files stay plain text for git, Play and the watcher; ending the session writes back first. A save of the file some other way (an older editor, a VS Code edit picked up by the host's folder) makes the document follow it as a diff.
- **Restarts** (`journal.rs`): each document's whole state, its origin, who typed last and the file's hash at the last write-back are kept in `.axcenity/teamcreate/live/` at each write-back and snapshot. Like `handled`, they outlive `discard`, so the next run of the host carries on with the same documents and members' copies merge back exactly (`a_host_that_crashes_carries_on_with_the_same_document`); a file changed while the host was down wins over the kept text. If the kept state is gone, the origin differs: a member's copy can't merge with the new one, so it starts over from the host's, and a member who typed something the file lacks makes their text again on top (only they do, so it isn't doubled). A clean end removes the folder.
- **Client** (`client.rs`): `open_doc`, `edit_doc`, `doc_text`, `take_doc_changes`, `undo_doc`, `redo_doc`. Offline typing stays in the copy and goes when it's back (`DocOpen` with its state vector after joining, then what the host lacks). Undo is `yrs`'s `UndoManager` over this member's own changes (others' arrive under another origin), so it never takes back someone else's typing.
- **Editor** (`app/team.rs` `team_live_docs`, `scripts.rs`): each open script or stylesheet tab in a session follows its document: typing goes in each frame, others' typing comes back with the cursor and selection moved past the changed span (`keep_cursor`), Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y in the tab go to the document, and these tabs never lock. A tab shows unsaved until the host writes the file; a write-back read from the folder only updates what the tab counts as saved.
- **Tests:** `live_tests.rs` (merges in any order, offline merges, character boundaries, bad updates, own-edit undo, 40 rounds of random typing on three copies with late and repeated updates), `live_session_tests.rs` (13 tests over the simulated network, every message through `wire`: two people typing at once, a viewer refused, too big / garbled / unopened changes, write-back by the typist, a save from elsewhere, offline typing, a host restart without and with its kept state, a late joiner, clean end, undo, message round trips), the editor's `a_joining_editor_gets_the_files_and_types_in_scripts_with_the_host` (two editors over real sockets type in one script, the host's file gets both lines, the joiner's Undo removes only theirs) and `scripts::others_typing_keeps_the_cursor_on_its_text`.
- **Cost** (`live_tests::typing_in_a_five_thousand_line_script`, debug build in the cloud): in a 5,000-line, 263 KB script a keystroke takes 0.04 ms to turn into an update and 0.005 ms to apply on another copy, and sends about 32 bytes; the whole document is 256 KiB, held once per open script by the host and by each member who has it open. The first version compared the texts a byte at a time and took 13 ms a keystroke; `changed_span` now compares 256-byte blocks.
- **Two machines over Tailscale (2026-11-12, `b0ff187`):** Paul's PC hosted (`--collab-host`) and the Mac joined (about 1 s); Paul typed in `pad.server.lua` on both at once: each saw the other's typing, neither tab locked, and he found it working. His one ask, seeing where the other person is typing as in Google Docs, is C8's cursors (§5.7), which that build didn't have yet.

## 5. Tailscale and the five-minute setup (C3, C4)

### 5.1 How Axcenity talks to Tailscale

Through the installed Tailscale client's command-line tool, never an embedded network stack (Tailscale's embeddable library `tsnet` is Go only). A new `collab::tailscale` module (no UI) does:

- **Find it**: `tailscale` on the PATH, else the standard install locations (`C:\Program Files\Tailscale\tailscale.exe`; `/Applications/Tailscale.app/Contents/MacOS/Tailscale`; `/usr/bin/tailscale`). Missing means the wizard offers the download page for the platform.
- **Read its state**: `tailscale status --json` gives the backend state (`NeedsLogin`, `Stopped`, `Starting`, `Running`), this machine's MagicDNS name and `100.x` addresses, the tailnet name and the signed-in user. Parsed into a small `TailscaleState` enum, tested with recorded JSON in the cloud.
- **Sign in**: `tailscale up` (or `login`) prints the sign-in URL, which the wizard opens in the browser; the wizard polls `status` until `Running` and then says so.
- **Check the path**: before the handshake, a TCP connect to the host's Tailscale name, with a plain-words result ("Your computer can't see Paul-PC on Tailscale yet. Ask Paul to share it with you").
- **Show who's who**: `tailscale whois` on a member's address shows their Tailscale login beside their Collaborate name in the roster. It is shown, never trusted for permissions.

The invite names the host by its MagicDNS name (falling back to its `100.x` address) instead of the LAN address, so links work from anywhere the joiner's Tailscale can reach.

**Built (C3, cloud):** `crates/collab/src/tailscale.rs`: `Cli::find` (`AXCENITY_TAILSCALE` path or `off`, the `PATH`, install locations), `state` and `parse_status` (`Missing`, `NotRunning`, `NeedsLogin` with the sign-in page, `NeedsApproval`, `Stopped`, `Starting`, `Running` with machine, MagicDNS name, addresses, tailnet and login), `Cli::start_login` (`tailscale up --json`, returns the sign-in page and leaves `up` waiting in the background), `whois`, `is_tailscale_host`, `check_reachable` (plain-words answers for a Tailscale name that doesn't resolve or a host that doesn't answer) and `advertise`, which points a `HostSession`'s invites at the Tailscale name. Everything that runs the tool goes through a `Runner` trait, so `tailscale_tests.rs` uses recorded output. The editor's Start Collaborate Session (panel and `--collab-host`) and `axcenity-collab` (unless `--address` or `--no-tailscale`) name the host by Tailscale when it's connected and say so; the panel shows "Tailscale: reachable as …", "Tailscale isn't connected: invites work on this network only", or, for a member, "Connected over Tailscale". Tests' own hosting stays on the LAN address. Still to do: recordings of real `status --json` from Windows and macOS replace the hand-written fixtures, the two-machine test, and the reachability check runs in the C4 join wizard (it blocks, so not on the UI thread).

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

**Found on Paul's PC (Windows, Tailscale 1.104.6, 2026-11-12):** `status --json` has every key the parser reads (`BackendState`, `Self.DNSName` with its trailing dot, `Self.HostName`, `Self.TailscaleIPs`, `CurrentTailnet.MagicDNSEnabled`, `User`); the CLI is on the `PATH` and at `C:\Program Files\Tailscale\tailscale.exe`. `tailscale up --json` on a machine that's already connected prints nothing and leaves it as it was, so the wizard only runs it when signing in is the next step. `axcenity-collab` advertised `<machine>.<tailnet>.ts.net:47810`, and the PC reached itself by that name over the Tailscale interface. Allow rules for the server appeared in Windows Defender Firewall (Private and Public) after its first listen, which is what the Defender prompt leaves when it's allowed (Paul to confirm he saw it): Plan A holds, and the setup guide tells people to choose Allow. The server's log printed the signed-in login, which is often an email address, and PowerShell 5.1 garbled its `›` and `…`: the line now names only the address, in ASCII.

**Found on Paul's Mac (macOS, Tailscale 1.104.6, same tailnet, 2026-11-12):** the CLI isn't on the `PATH`; only the app bundle's `/Applications/Tailscale.app/Contents/MacOS/Tailscale` exists, which `Cli::find` already checks. `tailscale ping` to the PC went direct over the home LAN (about 5 ms, no relay). The editor, started with the PC's owner invite as its only argument (C3's build, which still joined at once), fetched the project into its Collaborate folder and opened it with the session's World (checked through the editor's MCP connection: "Hello Touched", 3 objects in the Workspace, its script). **The C3 two-machine test passed.** Still open: whether the Defender prompt appeared on the PC (Paul to say), and the internet case across tailnets (node sharing), which needs a second account.

### 5.4 The setup wizard and Start Session (C4)

**Built:** `crates/editor/src/app/collab_setup.rs`. **File > Set Up Collaborate…** (and the first **Start** or **Join** on a computer) opens a wizard of four pages, Welcome, Tailscale, Check and Ready, in the editor's `dialog` pieces. The Tailscale page follows the installed Tailscale's state, asked by a background thread every two seconds while the wizard is open (never on the UI thread, nothing when it's closed): Install (opens the download page for the system), Open, Sign in (runs `tailscale up --json` on a thread and opens the sign-in page it gives), Waiting for approval, Turn on, Connecting; it moves on to Check by itself when Tailscale connects, and back if it disconnects. Check confirms the machine has an address others can use, that its MagicDNS name resolves to its own Tailscale addresses, and that port 47810 is free (both warnings, not blockers). **Same network only** skips Tailscale on any page. Each setup is timed per step on the computer only, and Output says "Setup took 2 min 10 s (installing Tailscale 1 min 40 s, signing in 29 s)", with a note when it went over five minutes. Finishing is kept in settings (`ready` in the `team_create` key), so later Starts go straight to **Start Collaborate Session**: the session's name (the project folder's by default), Edit or Only look, and how long the invite works; then **Your session is running** with the link, **Copy link**, and, over Tailscale, what someone with their own account needs plus **Share in Tailscale** (the admin console's Machines page; sharing has no CLI or API for this). Invite links carry the session's name (`&name=`, percent-encoded, at most 80 characters, control characters refused).

**Joining:** the start screen's Join window shows whose session a link is ("“Castle Game”, hosted by …") and, for a `.ts.net` host, that Tailscale must be on, before anything connects. While a join can't reach the host, the waiting line adds the usual causes for a Tailscale host in plain words. A bare link on the command line, as the system passes a clicked one, only fills in the Join window: joining waits for the user's click (the C4 security criterion); `--collab-join` still joins at once.

**Links open the editor:** the Windows installer registers `axcenity:` under `HKCU\Software\Classes` (removed on uninstall) and the Linux desktop entry declares `x-scheme-handler/axcenity` with `%U`. Not done: macOS delivers clicked links as an Apple Event, which eframe doesn't pass on, so the bundle doesn't claim the scheme yet; and a second launch opens a second editor instead of handing the link to the running one.

**Tests:** `collab_setup_tests.rs` (each Tailscale state gives one plain step, the timer, the checks finding a busy port, setup kept between runs, every page drawn and following Tailscale, Start Session hosting with the name in the invite) and the start screen's Join test (an offered link waits and says whose session it is). Snapshots of every page: `AXCENITY_SNAPSHOTS=<folder> cargo test -p axcenity-editor --bin axcenity-editor -- collab_setup_tests` (in a cloud container, install `mesa-vulkan-drivers` for a software adapter). The user's guide is `documentation/learn/collaborate.md`. The in-editor documentation viewer can't show images, so the guide is words and the wizard is the illustrated part; images in Learn pages are a later change to `start/docs`.

**Still to do in C4:** time a first setup by someone new to Tailscale on Paul's machines (the five-minute criterion), and a later session (under one minute).

### 5.5 Initial sync and recovery (C5)

**The double apply, fixed.** A host that wrote a member's edit to its journal and stopped before the confirmation left came back (new epoch) not knowing it had handled it; the member, from another run, renumbered every unconfirmed edit and sent it again, so it applied twice. Now the journal's folder keeps `handled` (the newest transaction number the host handled from each member: a line `member number` each), written with every snapshot and when a session ends, and folded in from the journal's entries when a recovered journal is discarded. `Host::start_journal` reads it back, so the next run's join message tells each member the newest of theirs it has; the client drops pending edits up to that number instead of re-sending them (they're in the snapshot it just took), and `Client::recover_unsent` leaves them out of a recovery file the same way. The file stays across clean ends, as a member's edits can reach a later session. Members are keyed by their stable member number from the members file.

**Checkpoints.** Hosting (the editor and `axcenity-collab`) keeps an unpinned Version History version "Before Collaborate session" (nothing new when the project is unchanged since the last one). A member whose editor sends edits made offline first keeps a pinned checkpoint of its copy, "Before sending changes made offline"; when some of them couldn't apply, the Recovered Changes window points at it with a **Version History…** button, where Compare shows what differs from the session. That's the "compare my copy with the session" view, reusing Version History's.

**Fault tests:** `journal_tests.rs` `edits_the_host_wrote_just_before_it_stopped_apply_once` (fails without the fix: the Part is added twice) and `a_recovery_file_leaves_out_what_the_host_already_has`; the editor tests check both checkpoints. A member offline across a host restart (`rejoin_tests.rs`) and other versions refused at the handshake (`net_tests.rs`) were already covered.

**Join time** (`join_time_of_ten_thousand_parts`, run by hand, release, cloud container): two members join a 10,000-Part World in 156 ms in-process; the join message is 205 KiB. Over Tailscale between Paul's PC and Mac the transfer adds the link's time; Paul's own projects are measured in the two-machine test.

**Left:** the two-machine cut test over Tailscale (C6 re-runs the convergence tests there). Recovery files stay in `.axcenity/teamcreate` inside the project folder, which never syncs or exports; moving them to the editor's data folder would change where TC6 recovery looks and wasn't needed.

### 5.6 Real-time scene collaboration (C6)

Scene sync itself was built in TC2–TC6 and needed no change. **Built:** the field-being-dragged bar left from TC4. While a Properties field is dragged, the editor puts its name in its presence's `editing` (already in the wire format, so no format change); the others' Properties show "▍ Paul is changing Transparency" in that member's colour, under "Also selected by", for the Instance they both have selected. It clears when the pointer is released. Tested in `team_tests::what_one_editor_selects_shows_in_the_others_session`.

**Two people over Tailscale (2026-11-12, `a5c5347`):** Paul's PC editor hosted from **File > Start Collaborate Session…** (the wizard went straight to its checks, Tailscale being connected) and the Mac's editor joined with the link (`--collab-join`): a direct path on the home LAN, 4 ms ping, the join itself about 1 s after the editor started (11 s), as an Editor, into its own Collaborate folder. Paul saw each editor list the other, a moved part move on the other screen, and the "is changing" bar while dragging a Properties value; the Mac's log had no errors. Paul found the wizard's Ready page drawn smaller than its other pages (its text sat beside the icon); it's laid out like them now.

**Left for the two machines:** the convergence and editor tests over Tailscale between Paul's PC and Mac, a 15-minute scripted session, and the added frame cost of 3 members dragging on S10k (`--bench` on the PC).

### 5.7 Chat and others' cursors (C8)

- **Chat** (`presence.rs`, `host.rs`, `client.rs`): `ToHost::Chat(text)` → `ToClient::Chat(ChatLine { member, at, text })` to everyone, the sender included, in the host's order. Any member may talk, Viewers too. The host tidies each line (`tidy_chat`: control characters other than line breaks and tabs left out, blank ends trimmed, at most 2,000 characters), refuses more than 5 lines a second from one member, and keeps the last 200 in memory only, never in the project or the journal; a member who joins gets them as `ChatHistory`. `Client::send_chat` checks the same before sending. Wire format 5.
- **Chat in the editor** (`app/team_chat.rs`): a Chat section in the Collaborate panel under Collaborators: each line with the name in that member's colour (hover for how long ago), plain text only, and any word that is the path of an Instance in the World now (`Workspace.Door`, found like the console's paths, punctuation around it left out) as a link that selects it. The `@` button puts the selected Instance's path in the message; Enter or the send button sends it, and a refusal ("Slow down a little", "Nothing to send", offline) shows under the box. Others' lines since the chat was last drawn show as a count on the title bar chip.
- **Cursors** (`Presence.script`, `scripts/others.rs`): while a live script or stylesheet tab has the focus, presence carries its selection's two ends as `yrs` sticky indexes (`Client::doc_cursor`), so they stay on the same text whatever is typed before them. Each other editor resolves them in its own copy (`Client::resolve_cursor`) and draws a caret in the member's colour with their name on a small tag (below the caret on the top row) and a light band over what they've selected. The host drops a cursor for a path that isn't live, longer than 512 bytes, or with sticky indexes over 64 bytes.
- **Tests:** `presence_tests.rs` (`chat_reaches_everyone_in_the_hosts_order`, `someone_joining_gets_the_last_two_hundred_lines`, `chat_is_plain_short_and_not_too_fast`), `live_session_tests.rs` (a cursor that stays on its text while someone types before it), the editor's `chat_reaches_the_other_editor_with_an_unread_count_and_instance_links`, `only_paths_of_instances_become_links`, the cursor part of `a_joining_editor_gets_the_files_and_types_in_scripts_with_the_host` (the joiner's caret shows in the host's tab and moves with its text) and `scripts::others_cursors_show_in_the_tab_with_their_names` (snapshot `script-others-cursors.png`).

### 5.8 Checkpoints and restores on the host (C9)

- **Where** (`history.rs`): in the host's project folder, `.axcenity/teamcreate/history/` beside the journal, which never syncs or exports; members, keys and certificates stay in the data folder as before. Each checkpoint is a folder named by its number: `world` (the World, written as a join message so Instances keep their IDs) and `info` (kind, label, who asked, who changed things since the checkpoint before it from the host's transactions, the transaction number, the project's files by hash, and a digest of the World and files). File contents go once each into `history/files/<hash>`, copied from the host's store the first time a checkpoint names them: the first checkpoint copies the project's synced files, later ones only what changed. A file the store can't give any more (the owner changed it on disk since) is named but not kept, and a restore leaves it as it is and says so.
- **When:** when the session starts (unless the newest checkpoint has the same World and files), every hour with changes (`CHECKPOINT_EVERY`, in `Host::tick`), when an Editor or Owner presses **Save** in the panel's History (`ToHost::Checkpoint(label)`, the `SaveCheckpoint` capability; one line, at most 120 characters), and before a restore. The newest 48 automatic ones are kept; saved ones and ones kept before a restore stay. Contents no checkpoint names are removed.
- **Restore** (`ToHost::Restore(id)`, the Owner only): the host keeps a "Before restoring …" checkpoint, works out one transaction (`history::restore_ops`) that makes the session's World and files what the checkpoint has, applies it and sends it to everyone like any other (label "Restore “…”"). The ops are worked out against a copy, each applied in turn, so moves land where they should: deletes of what the checkpoint doesn't have, then its tree parents first (moves and restores of what's still there, `CreateTree` for what was destroyed outright, at new IDs from the host's own block with everything pointing at them following), then names, properties, attributes and tags, then properties that point at other Instances, then the files (`FileVersion`, with the checkpoint's contents put back in the host's store). Live scripts follow their restored files. Members' undo stacks aren't touched: the restore is the host's step, and restoring the "Before restoring" checkpoint takes it back.
- **This differs from the plan** in two small ways: the checkpoints live in the project's `.axcenity` folder on the host, not the data folder (they're the project's content, like the journal, and travel with it), and members see the list and labels but don't open a checkpoint's contents; comparing is done on the host's project with Version History.
- **Members:** the list goes to everyone when it changes and to a joiner (`ToClient::Checkpoints`, newest 100); refusals and partial restores come back as `ToClient::HistoryNotice`. A joining editor keeps an unpinned Version History version of its copy as it joins ("Joined Collaborate session"), the mirror checkpoint left from TC6. Wire format 6.
- **Editor** (`app/team_history.rs`): the History section of the Collaborate panel: Save with a name, each checkpoint with when, who asked and who changed things, and for the Owner a restore button that asks once ("Restore for everyone").
- **Tests:** `history_tests.rs`: a restore of a World and files changed every way (a deleted door with an attribute, tag and an ObjectValue pointing at it, a moved and renamed child, a new part, a changed script, a deleted asset also gone from disk) gives the checkpoint's state on every copy in one transaction, and restoring the "before" checkpoint takes it back; Instances destroyed outright come back at new IDs with links kept; hourly and idle hours; Viewers refused and labels tidied; contents kept once by hash and old automatic checkpoints pruned; the messages over `wire`. The editor's `the_owner_restores_a_checkpoint_for_everyone_from_the_panel` (two editors over real sockets, snapshot `collaborate-history.png`).

## 6. Identity, authorization and the Django account manager (C10, C12)

Today: an `Identity` (name, colour, Ed25519 key) per machine, a members file on the host mapping public keys to roles, and invites that bind a new key to a role. Tailscale adds a *network* identity (the tailnet login) that is shown but never grants anything; the brief's "being on the tailnet doesn't mean you can edit" is already how the host works.

Seams (built in C10; no new service, no placeholder backend):

- `IdentityProvider` (`identity.rs`): the name and colour others see, the public key the host knows the member by, and signing. The handshake's `Knock` and `Remote` take any provider (`Arc<dyn IdentityProvider>`), so the host's checks don't change with the kind. One implementation now, `DeviceKeys` (the `Identity` this computer makes and saves). A later `AxcenityAccount` provider would sign in through the website (OpenID Connect with short-lived tokens the host verifies against the site's published keys).
- **Members keyed by a principal, not a key.** The members file has an optional `account` field next to the key (`access::Member::account`), written only when set and set by nothing yet; older files read as before. A future account can then claim existing device-key memberships (the host asks the Owner to confirm), so ownership, roles and the journal's attribution carry over; one account may own several device keys and a device key may be linked to no account, so the mapping is many-to-one, never assumed one-to-one with Tailscale logins.
- **Versioned integration points** listed in C12's document (sign-in, token refresh, profile, invite by account name, project ownership) with the website's API version in each call. Nothing calls them until the website exists.

### 6.1 Security review (C10)

Every way in was reviewed against the roles: the TCP listener, the handshake, and each `ToHost` message's handler in `host.rs`. Each finding is fixed or accepted below; the tests are `security_tests.rs`, `wire_fuzz_tests.rs` and two in `net_tests.rs`.

**Budgets** (`limits.rs`). The connection's 1,000 messages a second stays. Inside it, each kind that makes the host send a lot back, write to disk or use up something shared has a token bucket per member (a burst, then a steady rate). Over a budget, what's over is dropped, written once a minute in the log and the audit log, and the member's other kinds carry on. A dropped hello would leave the member waiting forever, so a hello over budget closes the connection instead, and the member reconnects after its back-off. Checkpoints and restores over budget get a notice ("asked for too often"). Downloads aren't dropped either: they wait in a queue for the budget, served by `Host::tick`.

| Message | Who may | Budget (burst, then per second) | Finding | |
|---|---|---|---|---|
| `Hello` | Any member | 20, then 1; over it the connection closes | Every hello with no `Have` sent the whole World and handed out a new ID block, without limit: a member could make the host send snapshots as fast as it liked and run the session out of IDs | Fixed |
| `Submit` | Per op: edit-world, edit-scripts or edit-assets | The connection's | Each is checked in full (capability per op, locks, uploads present, all or nothing, re-sends dropped). Dropping one would leave a copy out of step, and a member re-sends every unconfirmed edit at once after a reconnect, so there's no budget of its own | Accepted |
| `NeedIds` | Was anyone, Viewers too; now edit-world | 50, then 20 | No role check and no limit, and `IdAllocator::block` panicked once 2^31 indices were handed out (about 2 million blocks): any member could crash the host in about 35 minutes. Now it returns `None`: a Welcome then carries no block, and a restore says the session is out of IDs | Fixed |
| `Presence` | Any member | 60, then 30 | Each goes to everyone; the client sends at most 10 a second | Fixed (budget) |
| `Upload` | Edit-scripts or edit-assets for the path kind | The connection's; at most 4 files partway at once | Any number of uploads could be left partway, each up to 1 GiB in memory | Fixed |
| `Fetch` | Any member (Viewers need files) | 2 GiB, then 128 MiB a second, in bytes; at most 100,000 files waiting | Asking for the biggest file again and again made the host queue gigabytes a second for one member | Fixed |
| `Lock`, `Unlock` | Edit capability for the path | 100, then 20 | Each change goes to everyone | Fixed (budget) |
| `DocOpen` | Any member, live paths only | 500, then 20 (every open tab again after a reconnect) | Each answer can be a whole document | Fixed (budget) |
| `DocUpdate` | Edit-scripts (edit-assets for `.css`) | The connection's | Same reasoning as `Submit`; refusals now go in the audit log | Accepted |
| `Chat` | Any member | 5, then 5 | Already limited in C8; now through the same budgets | — |
| `Checkpoint` | Save-checkpoint (Editors, Owner) | 3, then one every 10 s | Each writes the World to disk: an Editor could fill the host's disk | Fixed (budget). Saved checkpoints aren't pruned: accepted, the Owner decides |
| `Restore` | Owner | 2, then one every 5 s | — | Fixed (budget) |
| `AuditLog` (new) | Owner | — | — | — |
| `HostCard` (Collaborate 2.0 P4) | Editors and the Owner (`Role::may_host`) | 5, then one every 5 s | Each changes the successor list, which goes to everyone; the address is trimmed, one word, at most 260 bytes | Fixed (budget) |
| `Tail` (Collaborate 2.0 P4) | Only a member a recovering host asked (`SendTail`), once | 3, then one every 10 s; the member's answer holds at most 16 MiB, oldest first (`client::TAIL_BYTES`) | Replayed without role checks (the old host accepted them), so only from a member asked, only entries continuing the host's own count, and only while recovering | Fixed |

**Before signing in** (`socket.rs`, `session.rs`):

- **Connections:** each one had a thread and no cap, and a connection that never said hello kept its place forever (the host's pings kept it alive). Now there are at most 256 connections, at most 64 partway through the handshake, and each has 10 s to sign in. Fixed.
- **Guessing invites:** an invite secret is 128 random bits, kept only as its SHA-256, so it can't be guessed. Even so, an address that fails 10 handshakes in a minute is now turned away for a minute, before any TLS work, and the audit log says so. Many addresses could still keep the 64 handshake places busy for 10 s at a time: that delays joining, it doesn't let anyone in. Over Tailscale only the tailnet (and machines it's shared with) can reach the port anyway. Accepted.
- **Replays:** each connection's challenge is a fresh 32-byte random value, and the signature covers it and the host certificate's fingerprint. A recorded proof doesn't work again on the same connection, on a new one, or at another host, and a used invite doesn't work for someone else (`a_replayed_sign_in_is_refused`). As designed in TC3; now tested.
- **Names:** a joiner's name could hold line breaks and control characters, which could put a fake line in the log or the audit log ("Eve⏎Paul joined as Owner"). Now each name is one line of at most 64 characters (`access::tidy_name`). Fixed.
- **Removed members:** their connection closes, their locks go, and the host ignores anything still on its way from them (`someone_taken_out_of_the_project_is_ignored` sends every message kind). As built; now tested.
- **Bad bytes:** the session decoders (postcard) and the handshake's JSON are fuzzed with random bytes and with real messages changed, cut short, lengthened and given huge lengths. Each test run does 20,000 rounds, and 1,000,000 ran once (`AXCENITY_FUZZ_ROUNDS`). Nothing panicked or allocated without bound; bad bytes are an error and close the connection. No fuzzing tool is needed (development policy).

**Viewers:** `a_viewer_changes_nothing_with_any_message` sends one of every `ToHost` kind (the test fails to compile when a new kind isn't in it) as a Viewer. It checks that the World, the files, the locks, the live documents, the transaction number and the checkpoints are all unchanged, and that nothing goes to anyone else but roster, presence and chat. Everything they asked for and may not have is in the audit log.

**Audit log** (`audit.rs`). The host writes down:
- joins and handshakes it refused, with the address;
- invites made and revoked;
- role changes and removals;
- restores;
- members refused for their role, or for going over a budget (once a minute per member and kind).

It keeps the newest 500 entries. It also appends each one to `<members file>.audit.jsonl` in the host's data folder (never the project's), readable only by its user where the OS allows, and moves the file to `.old` past 1 MiB. The Owner reads it:
- in the Collaborate panel's **Audit log** section, asked for when the section first shows and on Refresh (`ToHost::AuditLog`, wire format 7);
- with the server's `audit [count]` command.

The log is the host's own account: members can't check it.

**Accepted, not changed:**
- An Editor's transaction can be up to 32 MiB and is applied on the host's thread, so a hostile Editor can slow the session. Editors are trusted to edit, and removing them stops it.
- Uploaded files that no transaction uses stay in the host's `.axcenity/teamcreate/files` until the session ends.
- A device key can't be revoked apart from removing the member.
- On Windows, the members file, keys and audit log rely on the user profile's permissions.

**Never tripped in normal use:** the collab and collab-server test suites, and the editor's Collaborate tests (two editors over real sockets: joining, editing, imports, typing in scripts together, chat, restores), were run with any budget trip made a panic. Only the tests that flood on purpose tripped one. The busiest honest case seen is a member reconnecting seven times in a row in the bad-network convergence test, well inside the hello budget.

## 7. Other gaps found

- **Invite links don't open Axcenity.** Windows and Linux done in C4 (§5.4); macOS and handing a link to an editor that's already open are left.
- **No LAN discovery for sessions** (games have it). Not needed once Tailscale is the path; skipped.
- **Version History checkpoints of a member's mirror** were left from TC6. C9.
- **UI snapshots of the panel** were taken on Paul's PC in the hardware test; the wizard's are made by its tests (§5.4).
- **The double apply after a host stops** right after writing an edit (TC6 "Left"). Fixed in C5 (§5.5).

## 8. Phased plan C1–C12

One phase per priority in the brief, in order. Each is one or more PRs to `main`, tested locally (cloud first; Paul's PC and Mac only where the step needs real Tailscale or two machines, sequenced by the coordinator). Every phase writes its failing tests first (RED), then the code (GREEN), and updates this document, the topic docs and the wiki in the same PR.

| Phase | Builds | Tests (RED first) | Security | Performance | Done when |
|---|---|---|---|---|---|
| **C1** Audit | This document, ADR-0021 | `check-wiki.py` | — | — | Merged |
| **C2** Architecture and protocol | The protocol spec ([§9](#9-protocol-c2)): message kinds per content type, identifiers and recovery, versioning, batching (new message kinds land with the phase that uses them, at the end of the enums); the rename to Collaborate in names users see (old link and `--team-*` flags still accepted) | Wire round-trip tests for each new message; old invite links still parse; refusal text for an older wire version | Size limits for each new message kind from the start | No change to idle cost (measured with the existing idle-session test) | Spec reviewed, wire version bumped, rename merged |
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

## 9. Protocol (C2)

The session protocol as built by TC3–TC6, with the messages C3–C9 add. One rule holds throughout: **the host is the only place anything is accepted**; a client's copy is a cache it reconciles with what the host says.

### 9.1 Layers

1. **Transport:** WebSocket over TLS 1.3 on TCP (`socket.rs`), one connection per member, the host's self-signed certificate pinned by the fingerprint in the invite. A connection quiet for 15 s counts as lost. Over Tailscale (C3) the same connection runs inside Tailscale's WireGuard tunnel; Collaborate's TLS and signatures stay, so nothing depends on Tailscale for authentication.
2. **Handshake** (`handshake.rs`, JSON): hello (protocol `PROTOCOL`, engine version, build, class-defaults fingerprint, the host epoch the client last saw), the host's random challenge, the member's Ed25519 signature over the challenge and the certificate fingerprint, then admitted (with the role) or refused with a sentence for the joiner.
3. **Messages** (`wire.rs`): a format byte (`wire::FORMAT`), then postcard. `ToHost` from a member, `ToClient` from the host.

### 9.2 Messages by content

Per-member budgets for each kind are in [§6.1](#61-security-review-c10); the limits below are sizes.

| Content | Messages | Order and delivery | Kept | Checked by the host | Limits |
|---|---|---|---|---|---|
| Scene, components, properties, terrain, string cells | `Submit(Transaction)` → `Accepted { seq, … }` / `Rejected { client_seq, reason }` | Host sequence numbers; clients apply strictly in order and resync on a gap | Journal, then snapshot | Capability per op, classes, property types, tree rules, all-or-nothing | 32 MiB a message, 1,000 messages a second per connection |
| IDs for new Instances | `NeedIds` → `Ids(IdBlock)` | — | Per member in the host | — | 256 IDs a block |
| Asset and other files | `Upload` pieces (1 MiB) then a `FileVersion` op inside a transaction; `Fetch` → `File` / `Missing` | Content-addressed; the op is ordered like any transaction | Host's file store by SHA-256 | Capability by path kind, compare-and-swap on the base hash, path inside the project, extension allow-list | 256 MiB a file (configurable) |
| Structured documents (block scripts, graphs, materials, data tables, dialogues, behavior trees) | `Lock` / `Unlock` → `Locks` | Host grants one member at a time; a lock outlives a drop by one minute | Memory | Edit capability for the path | — |
| **Scripts and stylesheets (C7)** | `DocOpen { path, state_vector }` → `DocState { path, update, state_vector, origin }`; `DocUpdate { path, update }` up, `DocUpdate { path, update, author }` down; `DocRefused { path, reason }` | Any order: `yrs` merges converge; the host passes an update on only after applying it | Host's documents in memory, their whole state in `.axcenity/teamcreate/live/`, and the file it writes back | Edit-scripts (or edit-assets for `.css`) capability, the update decodes, size | 1 MiB an update; documents up to the per-file limit. Wire format 4 (5 from C8) |
| Presence | `Presence(Presence)` → `Presence { member, … }`, `Roster` | Latest wins; at most 10 a second per member | Memory (late joiners get the latest) | Selection trimmed to shared Instances, text clipped | — |
| **Text cursors (C8)** | Inside `Presence`: `script: Option<TextCursor { path, anchor, head }>`, the ends as `yrs` sticky indexes | As presence | Memory | Only live paths; path 512 bytes, each index 64 bytes | — |
| **Chat (C8)** | `Chat { text }` → `Chat { member, at, text }`; `ChatHistory` to a joiner | Host order | The host's last 200 messages, in memory only (never in the project or the journal) | Any member, Viewers included | 2,000 characters, 5 a second per member |
| **Version history (C9)** | `Checkpoint(label)` → `Checkpoints(list)` to everyone; `Restore(id)` (Owner); `HistoryNotice(text)` | Host order; a restore is one transaction for everyone | The host's `.axcenity/teamcreate/history`, file contents by hash | Save-checkpoint capability; restore is Owner only | Label 120 characters; 100 listed. Wire format 6 |
| Membership | `RoleChanged`, `Ended` | Host order | Members file in the host's data folder | Owner only for roles, removal, ending | — |
| **Audit log (C10)** | `AuditLog` → `AuditLog(entries)` | On request | The host's last 500 in memory, all in `<members file>.audit.jsonl` in its data folder | Owner only | Wire format 7 |
| **Members list (Collaborate 2.0 P3)** | `Members(MembersList)` | On joining, and to everyone who may host when it changes (its `version`) | Each member's project record in their Collaborate data folder (`records/<id>.json`) | Sent only to Editors and the Owner, never a Viewer | Wire format 8 ([collaborate-persistent.md](collaborate-persistent.md) §8.3) |
| **Host handoff (Collaborate 2.0 P4)** | `HostCard { address, cert }` up; `Successors { hosts, next_id }` to everyone when the list changes; `HandingOff { to, last_seq }` when the host leaves; `SendTail { after }` → `Tail(entries)` while a new host recovers | Host order; a recovering host replays tails in the old host's order, then the edits it held | Each member's last 2,000 accepted transactions of the current epoch, in memory | Cards from members who may host; tails only when asked | 8 successors. Wire format 9 ([collaborate-persistent.md](collaborate-persistent.md) §8.4) |

### 9.3 Identifiers, duplicates and recovery

- **Every edit has an identity:** (member, `client_seq`) from the client and `seq` from the host, within a host **epoch** (one run of the host). The journal stores the host's order; a re-sent `client_seq` that the host already applied is dropped, so nothing applies twice.
- **A member's copy says what it has** in its hello (`Have`: epoch and last `seq`). Same epoch: the host replays the journal from there. Another epoch, or older than the journal: a fresh snapshot, and the member's unconfirmed edits are re-sent with their new Instances moved into its new ID block.
- **Documents** catch up by state vector, not by sequence: whatever the member is missing comes back as one update, and whatever the host is missing goes up as one.
- **Nothing newer is overwritten silently:** a re-sent edit goes through the normal checks; refused or partly applied ones go to Recovered Changes, file saves that lose compare-and-swap become conflict copies, and offline text merges.

### 9.4 Versioning

- Any change to a message's shape raises `wire::FORMAT`; any change to the handshake or to what a message means raises `handshake::PROTOCOL`. Both, and the engine version and build, must match the host's in v1: a mismatch is refused with a sentence naming the versions, and the update chip fixes it.
- New message kinds go at the **end** of `ToHost` and `ToClient` (postcard numbers variants by position), and each new kind comes with a wire round-trip test and its limits in the same PR.
- When mixed versions become worth supporting, the hello will carry the highest format each side reads and the host will speak the lower; nothing does that yet.

### 9.5 Batching

- **World edits:** one transaction per frame from the World's edit journal (`capture.rs`); an open gesture (a drag) is held and sent at most 20 times a second, then once at the end.
- **Text:** `yrs` updates typed within one frame are merged (`yrs::merge_updates_v1`) into one `DocUpdate`.
- **Presence and cursors:** coalesced to the latest, at most 10 a second.
- **Large messages** (snapshots, big transactions, file pieces) of 16 KiB or more are packed with zstd level 1 when that saves at least an eighth (C11); small ones stay as they are.
- **Writes:** each connection writes every message queued since its last turn, then flushes once.

### 9.6 Performance (C11)

Measured on S10k (10,388 Instances, 200 scripts); the tables and how to run them are in [benchmarks](../benchmarks.md#collaborate-on-s10k-c11).

- **Packing.** A packed message starts with `FORMAT | 0x80` (format 10), then one zstd frame that must state its size. The receiver unpacks only up to its limit (`TO_HOST_MOST` 32 MiB, `TO_CLIENT_MOST` 1 GiB), so a small packed message can't fill its memory (`wire_fuzz_tests.rs` feeds it a bomb, a frame without a size and garbage). S10k's join message goes from 953 KiB to 82 KiB, for about 1 ms of packing.
- **Joining.** On one computer a member joins S10k in about 0.3 s and opens it as its project in 0.35 s more. The open took 5.9 s before C11 because loading any `world.json` was quadratic in a parent's children (each Instance looked up its prefab segment among all its siblings); now that lookup happens only inside prefab copies, and S10k loads in 0.19 s instead of 5.7.
- **Idle.** An idle session adds a mean of about 50 µs a frame to the hosting editor and 35 µs to a member (medians 15 and 3 µs); the goal was under 0.1 ms. Most of it is the folder check every 0.5 s (`Mirror::sync`, about 1 ms on S10k's 209 files), which finds saves made outside the editor. If a much bigger project makes that check show, it can move to a worker or follow the editor's file watcher.
- **Not built, by measurement.** Snapshot streaming (S10k's World is 82 KiB packed, about 70 ms at 10 Mbit/s) and more presence coalescing (four busy members are 19 KiB a second; editors already send at most 10 a second and only changes, and the host drops repeats). Both stay options for much bigger worlds or sessions.
- **Still to record.** A join of S10k over Tailscale between two machines (goal: under 10 s).

## 10. Risks

- **Tailscale is a third-party service.** If Tailscale is down or a user can't use it (blocked at work), LAN and a forwarded port keep working; nothing in sync depends on Tailscale.
- **Cross-tailnet sharing has a manual step** in Tailscale's admin console. The wizard can only open the page and explain it; automating it would need a Tailscale API key from the user, which is worse for beginners.
- **CRDT and the host's validation.** A CRDT update can't be half-refused. The host refuses whole updates (size, capability, decode) and never edits them, so all copies stay in step.
- **Rename churn.** Renaming every internal identifier would touch thousands of lines across open work. Only user-facing text, docs, flags and the link change now; internal names follow when those files are next edited.

## 11. Sources

- Tailscale, [Share your machines with other users](https://tailscale.com/docs/features/sharing), [Inviting users vs sharing a device](https://tailscale.com/docs/reference/inviting-vs-sharing), [Share a private game server with friends](https://tailscale.com/docs/use-cases/personal-or-at-home-use/share-private-game-server), [Tailscale Serve](https://tailscale.com/docs/features/tailscale-serve), [`tailscale up`](https://tailscale.com/docs/reference/tailscale-cli/up), [Windows firewall](https://tailscale.com/blog/windows-firewall) (all read 2026-11-12).
- `yrs` 0.28.1 on crates.io: [y-crdt/y-crdt](https://github.com/y-crdt/y-crdt) (MIT); dependencies checked against `Cargo.lock` at `d45d70b`.
