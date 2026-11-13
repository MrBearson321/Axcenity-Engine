# Axcenity Online: online services architecture (R19)

Roadmap item R19, which is section 7 (Phase 4) of [competitive-development-roadmap.md](competitive-development-roadmap.md). This is a **design document only**. The [development policy](../development-policy.md) says no infrastructure before it's needed, so nothing hosted is built here unless Paul asks for it. The document says how optional online services would fit the engine that exists, which seams are already in place, and the order to build them in if Paul decides to.

## 1. Summary

- **Every service is optional, and the engine never waits on one.** Making, testing, building and shipping a game work offline and without an account, as they do today. A service adds to a local feature. It never replaces one.
- **One website, one versioned API.** The Axcenity website (the Django project that already hosts the Toolbox) holds accounts, game pages, the asset marketplace, hosting and analytics behind one `/api/v1/`. The engine talks to it through one client crate, so turning it off is one setting.
- **Device keys stay the identity; accounts are a second fact about a key** (C12, [collaborate-accounts.md](collaborate-accounts.md)). Tokens are short-lived and bound to a device key. If the website is lost, nobody is locked out of a project.
- **Publishing reuses the engine's own release machinery.** A game's build gets a signed manifest in the same shape as the engine's release manifest ([release-system](release-system/README.md)). Its updates are the files whose hashes changed. The website is one place to host it, and the developer's own server or itch.io works the same way.
- **Hosting starts as a recipe, not a business.** Self-hosting `axcenity-server` comes first, with a container image, a health endpoint and a deploy guide. Managed hosting is a later decision made from measured per-player cost.
- **Analytics are off unless the developer turns them on, and never about the developer's players by default.** Only counts and performance are collected, never tracking. Retention is set by the developer, export is always available, and no data is ever sold.

## 2. What exists

| Area | Where | What it gives Online |
|---|---|---|
| Collaborate identity | `crates/collab` (`IdentityProvider`, `DeviceKeys`, `Member::account`, audit log) | Ed25519 device keys, a handshake, roles in the host's members file, and an unused `account` field (C10/C12) |
| Persistent Collaborate | [collaborate-persistent.md](collaborate-persistent.md) (P0–P7, merged) | Git/GitHub sync, checkpoints, host election and handoff. Section 7.2's "collaboration does not depend on one host" is done |
| Teamwork | [teamwork.md](teamwork.md) | Team Board tasks, bugs, goals and history in the project |
| Releases and updater | `crates/release`, `releases/`, [release-system](release-system/README.md) | A signed JSON manifest, channels, a daily check, a verified download and a separate updater that rolls back. This covers the engine only today |
| Build Game | [project-and-export.md](../project-and-export.md) | Packed `.axgame` builds, installers per OS, branding (icon and banner) and build profiles (R13) |
| Dedicated server | `axcenity-server`, [networking.md](../architecture/networking.md) | Headless server, LAN discovery, lag compensation, a per-player motion budget of 24,000 bytes a second |
| Toolbox | `crates/toolbox`, the Django Toolbox, [asset-sources](asset-sources/README.md) | Providers, the shared asset record, licenses and attribution, the local library |
| Logs and crashes | [developer-console.md](../developer-console.md) | `latest.log`, a panic hook that writes the crash to the log |
| MCP toolkit | [editor/mcp.md](../editor/mcp.md) | A typed tool surface, so a service's editor features can also be reached by AI clients |

## 3. Principles (from section 7, made testable)

- **P1. Offline is the default path.** Every feature that touches a service has a test that runs with the network refused. The editor opens, builds and plays with `online.enabled = false`, which is the default.
- **P2. No account to export.** Build Game never asks for one, and a built game never phones home unless the developer adds a service to it.
- **P3. The project folder holds no secrets.** API keys and tokens go in the user's data folder or the OS keychain, as Collaborate's keys do. A public repository leaks nothing.
- **P4. The developer owns data and builds.** Every hosted thing can be exported (game files, pages, analytics as CSV and JSON, assets) and deleted. Leaving the website takes nothing away from the project.
- **P5. One client, one switch.** All HTTP to the website goes through one `online` client crate with the versioned API, retries, offline caching and one setting. No other crate opens its own connection to Axcenity services. The Toolbox's existing providers keep their own sources.

## 4. Design per service

### 4.1 Accounts (section 7.1)

Use C12 as written: optional accounts on the website, tokens bound to device keys, linking members by signature, and roles kept by the host. R19 adds what C12 left to "when the website exists":

- **Sign-in from the editor.** The editor opens the system browser with an OAuth 2.0 authorization code flow and PKCE, and receives the code on a loopback port. The editor never sees a password. Refresh tokens live in the OS keychain (P3).
- **Profiles, teams and project links.** A team is a website object whose members are accounts. Linking a project to a team only lists it on the dashboard. Collaborate access is still decided by the host's members file (C12 §1).
- **API keys** are for servers and CI (hosting, analytics ingest). They are scoped to one game and one purpose, can be revoked, and are shown once.
- **Recovery and privacy.** Accounts are recovered by email or a recovery code, and a lost account never affects device keys. Every profile field except the display name is private by default.

### 4.2 Collaboration services (section 7.2)

These are built already: persistent projects, Git history, branches, checkpoints, recovery and host handoff (Collaborate 2.0), plus the Team Board. The online part left is an **optional relay** for teams that can't reach each other directly (no Tailscale, strict NAT). It forwards the existing TLS Collaborate stream (pinned to the host certificate's fingerprint) without being able to read it, and admits by the same handshake. It is the self-hosted collab server run by the website. Until the website exists, the setup guide covers Tailscale and self-hosting.

### 4.3 Game publishing (section 7.3)

- **A game release is a manifest plus files.** Build Game already writes the files. A new step writes `game-manifest.json`, with the same shape and signing as the engine's release manifest: version, channel, each file's path, size and SHA-256 hash (`sha2`, already pinned for the release manifest), and an Ed25519 signature by a **game key** kept in the developer's data folder.
- **Updates are the changed files.** The player (or a small launcher) compares hashes and downloads only the files that changed. It verifies the signature and swaps them with the engine updater's rollback. This meets "updates do not require rebuilding the entire distribution".
- **Hosting is pluggable.** The manifest names a base URL. Publishing can upload to the website, to any static host (S3-compatible, GitHub Releases, the developer's server), or to nothing, leaving a folder to zip for itch.io or Steam. The editor's **Publish** dialog picks one.
- **Game pages** on the website use the branding the build already has (icon, banner), with a description, screenshots, versions, public, unlisted or private visibility, and download counts. Download counts are the only number collected by default.

### 4.4 Hosting (section 7.4)

- **Stage A, self-hosting made easy** (useful without the website): an official container image of `axcenity-server`, a `/health` endpoint, structured logs, graceful restart, and a deploy guide for one VPS. The server already runs headless.
- **Stage B, bring your own machines:** the dashboard registers servers by API key. A small agent reports health and version and does rolling updates from the game manifest (4.3). The website orchestrates without owning the hardware.
- **Stage C, managed hosting**, a business decision made only after B. The infrastructure questions in section 7.4 are answered from measurements, not guesses. What to measure first:
  - CPU per player per tick.
  - RAM per server and per player.
  - Bytes per player per second (the motion budget gives the ceiling: 24,000 bytes a second plus reliable updates).
  - Start time.
  - Crash rate.

  `axcenity-server` with the S10k and character benchmarks gives these numbers on Paul's PC. The open questions are bare metal or cloud, core density, transit cost, DDoS protection, regions, container orchestration and isolation. Each is weighed against those numbers in a later document. Isolation means one game per container, no shared filesystem, and egress limits.

### 4.5 Analytics (section 7.5)

- **Off by default, per game, opt-in by the developer.** When it's on, the game's settings screen tells players and gives them an opt-out. No data leaves a game whose developer didn't turn it on.
- **What's collected:** session start and end, concurrent players, session length, frame time and server tick summaries, crash reports (the panic hook's message and log tail, never files), latency, and developer-defined events (`AnalyticsService:Track(name, number?)`). Each event carries counts and numbers only. There's no free text and nothing that identifies a person. A session ID is random per session.
- **Retention** is set by the developer (default 90 days). Export is CSV and JSON, and deletion is one button.
- **Ingest** batches events on the client and sends them through the `online` client at most once a minute. Events are dropped when offline beyond a small cap. Analytics can't slow a game.

### 4.6 Asset marketplace (section 7.6)

This builds on the Toolbox: the website becomes one more **provider**, the "Axcenity" source, behind the existing provider interface. Its records already carry license, author and source URL.

- **Publishing an asset** uploads an asset package (`.axpkg`, [packages.md](../project/packages.md)) with a version, plus license and attribution fields that the website requires (the package format gains them as optional fields). They are shown in the Toolbox before install, as provider records' licenses are now.
- **Ratings, reviews, profiles and download counts** live on the website. The local Toolbox and its local library keep working without it (P1).
- **Paid assets** are out of scope until accounts, payouts and tax are a decided business. The record leaves room for a price.

### 4.7 Developer dashboard (section 7.7)

This is a website section, not an editor copy. It covers account and team, games and releases, servers (stage B or C), analytics, published assets and API keys. The editor links to each page rather than redrawing it. The editor keeps only what belongs in the work loop: publish, server status and a crash list.

## 5. The `online` client (the only engine code R19 would add)

- A new crate, `crates/online`, that depends on `core` and on nothing in rendering or the editor. It holds the API types (generated from the website's OpenAPI schema), auth (PKCE, refresh, keychain), retries with backoff, and an offline cache.
- The editor, the player (publish updates, analytics) and the server (health, analytics) use it. The setting `online.enabled` is off by default. When it's off the crate makes no connections, and a test proves it.
- The website's API is versioned. The engine sends its version and the API answers with what it supports, as the Collaborate handshake does with protocols.

## 6. Threats

C12 §9 covers tokens. Online adds:

- **A stolen game key** lets someone sign fake updates. The key lives outside the project, can be rotated (the manifest lists the next key's fingerprint), and the website's copy of a page can pin the key.
- **A malicious update host** can only serve files whose hashes are in a signed manifest. A wrong hash is refused, and the player keeps the old files.
- **An analytics flood** is limited by per-key rate limits and server-side caps. Events are numbers, so nothing a player types reaches the developer.
- **Marketplace malware.** Packages hold assets, not executables. Scripts inside a package are shown before install, as the Toolbox does, and the website scans uploads.

## 7. Order of work, if Paul says go

| Step | What | Needs the website? |
|---|---|---|
| O1 | `game-manifest.json` with a game key, delta updates and the rollback, published to a folder or a static host | No |
| O2 | `axcenity-server` container image, `/health`, structured logs and a deploy guide (hosting stage A) | No |
| O3 | Website API v1, accounts, sign-in from the editor, the `online` crate | Yes |
| O4 | C12 account linking in Collaborate, the optional relay | Yes |
| O5 | Game pages and Publish to the website | Yes |
| O6 | Marketplace provider and asset publishing | Yes |
| O7 | Analytics (opt-in) and the crash list | Yes |
| O8 | Server agent and dashboard (hosting stage B) | Yes |
| O9 | Managed hosting (stage C), after its own cost document | Yes, and a business decision |

O1 and O2 help every developer now and need no website, so they are the candidates if any of R19 is built early.

## 8. Acceptance (section 7) and how this design meets it

- **The engine is fully usable without services:** P1, P2 and §5, with an offline test.
- **Collaboration does not depend on one host:** done by Collaborate 2.0. The relay is optional.
- **Publish without surrendering ownership, and distribute independently:** §4.3 (any host, the developer's key).
- **Updates don't rebuild everything:** §4.3, which downloads only changed files.
- **A straightforward server deploy, with self-hosting supported:** §4.4 stages A and B.
- **Developers control what is collected:** §4.5 (off by default, numbers only, retention, export).
- **Asset licensing visible, attribution kept, the local Toolbox works offline:** §4.6.
