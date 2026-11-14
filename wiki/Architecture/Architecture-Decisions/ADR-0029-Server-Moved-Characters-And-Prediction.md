---
title: "ADR-0029: Server-moved characters from numbered moves, predicted and reconciled on the device"
type: adr
adr: 29
adr_status: Accepted
date: 2026-11-14
supersedes:
superseded_by:
affects: [net, character, server, player, editor]
source: [crates/net/src/predict.rs, crates/character/src/predict.rs]
docs: [docs/research/multiplayer-readiness.md, docs/architecture/networking.md]
tags: [adr, networking, characters]
---

# ADR-0029: Server-moved characters from numbered moves, predicted and reconciled on the device

- **Status:** Accepted (Paul asked for general-purpose prediction and rollback on 2026-11-14; this is the plan's D3, stage MP6a, refined in building it, and can be changed by him)
- **Date:** 2026-11-14
- **Affected systems:** the character controllers, `Network`, `axcenity-server`, the player, editor Play, the bots

## Context and problem

Players' characters are moved by their own devices and checked by the server (Phase 66, MP3b's wall and flight checks). A modified client can still do anything the checks allow, and a check can only refuse a move after the fact. Paul wants prediction and rollback for public games. The development policy rules out cross-machine determinism (no fixed-point, no `enhanced-determinism`), so lockstep and exact rollback are out; the server must stay the authority, and devices must not feel their own latency.

D3 in the plan named fixed numbered ticks shared by server and players. The engine's frame is variable, and characters step with the frame's `dt` (physics alone runs a fixed 60 Hz).

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Numbered moves of the device's own lengths: the server takes the same steps, paid from a real-time allowance | No change to the frame loop or the character code's `dt`; the server's step is exactly the device's | The server must cap how much time a device can claim |
| Fixed ticks for characters (D3 as written), several per frame | Same step on every machine; easier to line up with physics | A second clock inside the variable frame; characters step at a rate the camera doesn't |
| Keep client-moved characters and only tighten checks | Nothing new | Checks are after the fact; a cheat within the limits stays |

## Decision

`Players.CharacterAuthority` chooses: `"Client"` (default, unchanged) or `"Server"`. Under `"Server"` a joined device records each step of its own character as a numbered `Move` (length, the camera's turn, keys, stick), sends the unanswered ones redundantly on the unreliable channel, and keeps what it predicted for each. The server takes each player's moves in order with the same controller code (`Controllers::drive`), paying them from an allowance of real time capped at half a second, and answers with the character's state after the last move. A device whose prediction differs rewinds to the answer, takes its later moves again, and eases the shown difference away (a 0.05 s half-life; over 4 studs it snaps).

## Reasons

- The server's step and the device's are the same function on the same inputs, so on the same machine type they agree exactly and corrections come only from what the two worlds disagree on.
- No fixed tick is needed for characters; the allowance keeps a device from claiming more time than passes, which is what a tick would have bounded.
- Client authority stays the default, so existing games and the Phase 66 checks are untouched.

## Consequences

- The network protocol changes (`PROTOCOL_VERSION` 15).
- Other players are drawn in the past on a device, so collisions with moving players cost corrections; moving platforms aren't rewound.
- Seats, `MoveTo` and `MoveDirection` don't drive a remote player's character under `"Server"` yet; owned assemblies stay client-moved until MP6b.
- Revisit if physics bodies need prediction (MP6b): those step at physics' fixed rate and would need rapier state saved and restored.

## Implementation references

- Plan: `docs/research/multiplayer-readiness.md` D3 and MP6a
- `crates/character/src/predict.rs`: `Drive`, `CharacterState`, `Controllers::take_drive`, `drive`, `drive_again`, `state`, `set_state`
- `crates/net/src/predict.rs`: `Move`, `Moved`, `Predictor` (device), `Driven` (server), `PredictionStats`; `Network::step_characters`, `record_move`, `prediction` in `game.rs`
- Tests: `crates/net/src/predict_tests.rs` (emulated 100–200 ms with jitter and loss, a modified client claiming time, a server teleport)
- `axcenity-server --character-authority server`; the bots print prediction stats
