# Collaborate accounts: integrating a future Django account manager (C12)

Paul's Collaborate brief (2026-11-12) asks for identity that works today with no website, and that a Django account manager on the Axcenity website can join later without reworking the host. This document is C12 of [collaborate.md](collaborate.md) §8: how that integration would work. **Nothing here is built.** There is no Django backend, no hosted service, no placeholder endpoint and no code that calls one. What exists are the seams from C10 ([collaborate.md §6](collaborate.md#6-identity-authorization-and-the-django-account-manager-c10-c12)).

- **Status:** design only, for when the website exists. Each part names the engine change it would need.
- **Related:** [ADR-0021](../../wiki/Architecture/Architecture-Decisions/ADR-0021-Collaborate-Tailscale-Live-Scripts-And-Identity-Seam.md) (identity seam), [collaborate-persistent.md](collaborate-persistent.md) and ADR-0022 (the members list and ownership that travel with the session, the project ID).

## 1. Summary

- **Accounts add to device keys; they never replace them.** Every computer keeps its Ed25519 key, and the host keeps admitting members by key. An account is a second, optional fact about a key: "this key belongs to account A".
- **The website proves who someone is. The host decides what they may do.** Roles stay in the host's members file (or, with Collaborate 2.0, in the members list that travels with the session). The website never grants editing by itself.
- **Tokens are short-lived, for one project, and bound to one device key.** A token stolen from a computer, a log or a malicious host is useless without that computer's private key.
- **Losing the website never locks anyone out.** If it's down or gone, members still join by key; only account features wait.
- **Tailscale logins, device keys and accounts are separate.** One account may own several device keys; a key may have no account; a tailnet login is never mapped to either.

## 2. What exists (C10)

| Seam | Where | What it holds now |
|---|---|---|
| `IdentityProvider` | `crates/collab/src/identity.rs` | Name, colour, public key, signing. One implementation: `DeviceKeys` (the `Identity` each computer makes). The handshake (`Knock`) and `Remote` take any provider |
| `Member::account` | `crates/collab/src/access.rs` | `Option<String>`, written only when set, set by nothing yet. Older members files read unchanged |
| Handshake | `crates/collab/src/handshake.rs` | Hello (versions, key, name, colour, invite) → random 32-byte challenge → signature over a fixed context, the challenge and the host certificate's fingerprint → admitted or refused. Protocol 5 |
| Audit log | `crates/collab/src/audit.rs` | Joins, refusals, invites, roles, removals, restores, denials and limits, beside the members file, Owner only |
| Where secrets live | The host's data folder, never the project folder | Members, keys, certificates and invites' secret hashes. The project folder and its repository may be public |
| Project ID | `collaborate.json` (Collaborate 2.0, P3) | A stable project ID, the name and the Git remote; no secrets |

## 3. Providers

**Device keys** (today). The member is their key. Name and colour are what the member says.

**Axcenity account** (later). The website is an OpenID Connect provider (Django with a maintained OIDC provider package; the site may itself let people sign in with GitHub, Google or email). The editor signs in once per computer and from then on asks the website for a **session token** each time it joins a project. The provider:

- still signs the handshake with the computer's device key (the private key never leaves the computer and is never sent to the website);
- adds an account token to the hello;
- takes its display name from the account profile (others can still see the key's fingerprint).

Engine changes when built:

- `IdentityProvider` gains `fn account_token(&self, project: &str) -> Option<AccountToken>`, which `DeviceKeys` answers with `None`. Fetching a token is network work, so the editor fetches it before knocking (on a worker, like update checks) and the provider only hands it over.
- `Greeting::Hello` gains `account: Option<String>` (the token as sent), which raises `handshake::PROTOCOL`. The signature the joiner sends then also covers the token's SHA-256, so a token can't be lifted from one connection to another.
- The host gains an `AccountCheck` (§4), used by the handshake's gate after the key's signature checks out.
- Other providers (a studio's own OIDC server, say) are the same shape with another issuer. Which issuers a host accepts is a host setting, empty by default.

## 4. Token checks on the host

A session token is a JWT signed by the website. The host checks, in this order, and refuses the account part (not necessarily the join; §4.2) if any check fails:

| Check | Rule |
|---|---|
| Algorithm | `EdDSA` or `ES256` only, from a fixed list in the engine. `none` and every `HS*` are refused (no algorithm confusion) |
| Key | `kid` names a key in the issuer's JWKS, fetched over HTTPS with ordinary certificate checks and cached by its `Cache-Control`, at most 24 hours. An unknown `kid` refetches once, at most once a minute |
| Issuer | `iss` equals an issuer the host accepts, exactly |
| Audience | `aud` is `axcenity-collaborate:<project ID>`, the project this host is hosting |
| Time | `exp`, `nbf` and `iat` with 60 s of leeway on the host's own clock; a lifetime (`exp − iat`) over 10 minutes is refused |
| Key binding | `cnf.jkt` (RFC 7800, with an RFC 7638 thumbprint) equals the thumbprint of the public key the joiner just proved it holds |
| Subject | `sub` is the account's opaque, stable ID (never an email). It becomes `Member::account` |
| Replay | `jti` is remembered until `exp`; a second use is refused |

Tokens are checked at sign-in only. A member whose account is later disabled keeps the session until they leave or the Owner removes them, as a removed member does today; their next sign-in fails the account check.

### 4.1 Offline and outages

- The host needs the website only to fetch the JWKS. With a cached JWKS it checks tokens with no network.
- If the cache is empty or expired and the website can't be reached, the host admits members **by key only**, as today, and says so in the panel and the audit log ("Accounts couldn't be checked: the website didn't answer"). Account links already in the members file stay; nothing new is linked.
- A computer that can't reach the website joins by key only. Nothing about joining waits on the website.

### 4.2 What an account check decides

By default an account check never lets in someone the members file wouldn't, and a failed check never shuts out a member by key. It decides only:

- whether to link the key to the account (§5);
- what name to show (the account's, when linked);
- with **"Members must sign in with an account"** on (a later host setting, off by default), whether a key with no valid account token is refused, with a sentence saying why.

## 5. Linking existing members to accounts

Members made before accounts exist keep working. Linking is per key, per project:

1. Sam signs in to the website in Settings (once per computer).
2. Next time Sam joins, the hello carries a token for account A bound to Sam's key K.
3. The host knows K as member 7 with no account. It asks the Owner in the Collaborate panel: "Sam (key 3F2A…) signed in as **sam** on the Axcenity website. Link this member to that account?" The Owner can choose **Link automatically** for the project instead.
4. On yes, `Member::account = Some(A)`, an audit entry says who linked what, and with Collaborate 2.0 the link travels with the members list to every would-be host.

Rules:

- A key is linked to at most one account. Changing it needs the Owner, and both changes go in the audit log.
- One account may have several keys (a desktop and a laptop). **Adding a device**: a new key with a valid token for an account that already has a membership in this project may join without an invite, with that membership's role, **only** when the Owner turned on "Let signed-in members add their devices" (off by default; §9 says why).
- The Owner or the member can unlink. Unlinking keeps the membership by key.
- Removing a member removes that key. With "Remove all of this account's devices" the Owner removes every key linked to the account.

## 6. Ownership

- **The members file stays the authority.** The Owner role is a role on a key (and, once linked, on that key's account). Changing who owns a project is the existing role change, made by an Owner and recorded in the audit log.
- **The website's project record is a listing, not a grant.** An Owner signed in to the website may register the project (by its ID from `collaborate.json`) so their account lists it and others can find it. The claim carries a signature by an Owner key over `("axcenity collaborate claim v1", project ID, account ID, time)`. The website stores it but can't check it against a members file it never sees; hosts can. A host shows the website's record only when the claim's key is an Owner in its own members list; otherwise it's ignored and the panel says the listing doesn't match.
- **Transfer:** the new Owner is made Owner in the session first (as today), then re-registers the listing with their own claim. The old record stays until replaced.
- **Migrating today's projects:** nothing to migrate. A project with no listing works exactly as now; registering adds a listing, linking adds accounts, and both can be undone.

## 7. Roles and attribution

- **Roles** stay Owner, Editor and Viewer, held by the host. The website may show a copy for display only.
- **Transactions** are attributed by `MemberId` (as now); the members file maps the ID to its key and account, so history can show the account's name once linked.
- **Checkpoints and the audit log** keep the name they recorded at the time; nothing is rewritten when someone links an account later. Audit entries gain the account ID next to the member ID.
- **Git commits** (Collaborate 2.0 P1–P2): a linked member's commits are authored with their account's display name and the website's no-reply address for that account; an unlinked member's with their name and a local no-reply address. No real email address is written unless the member chooses it.

## 8. The website's API (versioned)

What the Django site would offer the editor. JSON over HTTPS. Every call sends `Axcenity-Api-Version: 1` and the engine version; the major version is also in the path. Within v1, fields are only added. A response the editor doesn't understand turns account features off with a sentence; joining by key still works.

| Call | Purpose |
|---|---|
| `GET /.well-known/openid-configuration`, `GET /.well-known/jwks.json` | OIDC discovery and the token-signing keys the host caches |
| Authorization code with PKCE in the system browser, redirect to `http://127.0.0.1:<port>/` (RFC 8252); `POST /oauth/token` | Sign-in. Scopes `openid profile collaborate`. No embedded web view |
| `POST /oauth/token` with `grant_type=refresh_token` | Token refresh. Refresh tokens rotate, and a reused one revokes the family. Kept in the OS keychain, never in the project folder or the data folder's plain files |
| `POST /api/collaborate/v1/session-tokens` `{ project, key_thumbprint, key_proof }` | A session token for one project, bound to one device key (`key_proof` is the device key's signature over the request, so the website only issues `cnf` for keys the caller holds). Lives 5 minutes |
| `GET /api/collaborate/v1/me` | Profile: account ID, display name, avatar URL |
| `GET /api/collaborate/v1/me/devices`, `DELETE /api/collaborate/v1/me/devices/{thumbprint}` | The keys this account has asked tokens for; removing one stops new tokens for it |
| `GET /api/collaborate/v1/accounts?handle=…` | Find an account by its handle to invite it: ID and display name only, rate limited |
| `POST /api/collaborate/v1/invites` `{ project, account, role, expires }` | An invite addressed to an account: the Owner's editor makes an ordinary invite and asks the website to deliver its link to that account. The website holds the link for delivery only; the host still checks the invite's secret |
| `POST /api/collaborate/v1/projects`, `GET /api/collaborate/v1/projects/{id}`, `PUT /api/collaborate/v1/projects/{id}/owner`, `DELETE /api/collaborate/v1/projects/{id}` | The listing (§6); each change carries an Owner key's claim |
| `POST /oauth/revoke` | Sign out on this computer |

The host itself calls only the JWKS. Every other call is the editor's, on behalf of the person using it.

## 9. Threat model for tokens

| Threat | What stops it |
|---|---|
| A token is copied from a computer, a log, a crash report or the network | Bound to one device key (`cnf.jkt`): the host also requires a signature from that key over this connection's challenge. Five-minute life, one use (`jti`). Never logged; the audit log keeps the account ID only |
| A malicious or compromised host collects tokens to use elsewhere | Each token names one project (`aud`) and one key, and is used once. The host never sees refresh tokens or the key's private half |
| A token for one project is replayed to another, or to the same project over another connection | `aud` is the project ID; the handshake signature covers the challenge, the host certificate's fingerprint and the token's hash |
| A forged token | Signature checked against the issuer's JWKS fetched over HTTPS; a fixed list of algorithms (no `none`, no `HS*`); exact `iss` |
| The website is compromised and mints tokens for any account | It still can't add anyone to a project: membership is by key, and links need an Owner's yes. With "add their devices" on, an attacker with forged tokens could add a key under a linked account, which is why it's off by default and every such join is in the audit log |
| A refresh token is stolen from a computer | Kept in the OS keychain; rotation with reuse detection revokes the family; signing out revokes it; the website's device list removes a key |
| A token or account ID ends up in a public repository | Tokens are never written to the project folder; `collaborate.json` holds no secrets; account IDs are opaque and, like member keys, live only in the host's data folder |
| The website is down or gone | Hosts admit by key; nothing waits on the website (§4.1) |
| Clocks disagree | 60 s leeway on the host's clock; a refused token says the clocks may differ |
| Someone phishes the sign-in | System browser with PKCE and a loopback redirect, so the editor never sees the password and a stolen code is useless without the verifier |
| Probing account handles to learn who uses Axcenity | The lookup returns only what a profile shows publicly, and is rate limited |
| Tailscale and account identities are confused | Tailscale is never used for permissions, and nothing maps a tailnet login to an account |
| An account is deleted | `Member::account` holds an opaque ID; the membership by key stays and shows the last known name |

## 10. Order of work, when the website exists

1. The website's OIDC provider, JWKS and `me` (no engine change).
2. Settings > Account: sign in, the keychain, the profile's name (editor only).
3. Session tokens and the host's `AccountCheck`, the protocol bump, linking with the Owner's yes, audit entries, `account` in the replicated members list.
4. Invites by account, the project listing, Git authorship from accounts.
5. Optional host policies: require accounts, let signed-in members add their devices.

Each step ships on its own and keeps joining by key working. Tests would cover every row of §4 and §9 with a local test issuer (a JWKS served from the test), never a real website.

## 11. Sources

- OpenID Connect Core 1.0 and Discovery 1.0 (openid.net/specs).
- RFC 7519 (JWT), RFC 7517 (JWK), RFC 7638 (JWK thumbprint), RFC 7800 (proof-of-possession `cnf`), RFC 8725 (JWT best current practices: algorithm allow-lists, `none` refused).
- RFC 7636 (PKCE), RFC 8252 (OAuth 2.0 for native apps: system browser, loopback redirect), RFC 9700 (OAuth 2.0 security best current practice: refresh-token rotation).
