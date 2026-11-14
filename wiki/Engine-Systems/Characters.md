---
title: Characters
type: system
status: current
implementation: implemented
crates: [character]
source: [crates/character/src/lib.rs, crates/character/src/seat.rs, crates/character/src/rig.rs]
docs: [docs/scripting/api-character.md, crates/character/CLAUDE.md]
verified: 2026-11-11
verified_commit: 19c7179d
tags: [system/character, gameplay]
---

# Characters

> [!summary]
> `character` steps `ThirdPersonController` and `FirstPersonController` Instances (declared in `core`) with the same **kinematic** movement code: ray probes (`physics::cast_ray_excluding`), not a physics body. Walk, run and crouch are built-in modes; jump, slopes and step-up are movement on those modes; a custom `Movement` name hands motion to scripts. It also seats characters (Seats and VehicleSeats), rides anchored moving platforms, and provides the default character rig. Humanoids, Players and spawning live in `script` on top of it. **Implemented.**

## Architecture and flow

```mermaid
sequenceDiagram
  participant H as Host (frame step characters.step)
  participant C as Controllers
  participant P as physics (queries only)
  H->>C: MoveInput::from_actions(input map)
  C->>C: update_seats, Footing/carry (ride anchored part)
  C->>P: cast_ray_excluding (ground, walls, steps)
  C->>C: walk/run/crouch, jump, fall by Workspace.Gravity, camera
  H->>C: take_seat_changes, take_landings → ScriptRuntime::fire_seated / fire_landed
  Note over H,C: later, physics.kinematic: keep_kinematic anchors the parts it moves
```

- The controller of a Player's `Character`, else the first controller in the Workspace with a body, receives input and owns the camera; others walk their `MoveDirection` (Humanoids).
- A seated character is unanchored and welded to its seat; `keep_kinematic` skips it.

## Dependencies

`core`, `anim` (seat `Sit` state), `assets`, `physics`. **`physics` must never depend on it**; `nav` may.

## Performance

**Measured** (S10k, 9950X3D): Characters was 2.70 ms with one player character (ray probes against 10,000 parts); after L4's kept query bounds the test runner's Characters step is 0.31 ms (cloud container). See [Optimization log](../Performance/Optimization-Log.md#l4-query-bounds).

## Testing

`crates/character/src/tests.rs`, `tests/seat.rs`; player tests `seat_tests.rs`, `vehicle_tests.rs`, `spawn_tests.rs`, `humanoid_tests.rs`, `default_humanoid_tests.rs` (the default humanoid asset).

## Development guidance

- Never change physics settings from here; fall by `physics::gravity(world)`.
- A new movement feature works in both controllers (only `Eye` and `pose_camera` differ).
- The default rig is still six welded blocks. The skinned default humanoid (`assets/characters/humanoid`, `examples/hello-humanoid`) replaces it in stage HD3 of `docs/research/default-humanoid.md`: its collision is an invisible root part apart from the visuals, since a MeshPart in an A-pose would make the body's radius (from its parts' extent) far too wide.

## Related documentation

`docs/scripting/api-character.md`; [ADR-0007](../Architecture/Architecture-Decisions/ADR-0007-Fixed-Step-Physics-And-Kinematic-Characters.md); [Physics](Physics.md); [Animation](Animation.md)
