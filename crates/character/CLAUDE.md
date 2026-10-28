# `character`

- Kinematic character controllers. `ThirdPersonController` and `FirstPersonController` are Instances in `core`; this crate steps both with the same movement code (only `Eye` and `pose_camera` differ). It may query `physics`. `physics` must not depend on this crate.
- Behavior: `docs/scripting/api-character.md`. Frame order: `docs/architecture/frame-loop.md`.
- Hosts build `MoveInput` with `MoveInput::from_actions` (the input map's Move, Look, Jump, Run, Crouch), call `Controllers::step` after input and before scripts, then `keep_kinematic` immediately before the physics step. The first controller in the Workspace with a body receives movement and owns the camera.
- Movement is ray probes (`cast_ray_excluding`), not a physics body. Falls by `physics::gravity(world)` (`Workspace.Gravity`); a jump launches at `JumpHeight`'s speed under the default `GRAVITY`. Never change physics settings from here. Tests are in `src/tests.rs`.
- A custom `Movement` name (not Walk, Run, Crouch or `""`) returns from `step` before any motion; `""` restores the built-in mode remembered in `Controllers::built_in`.
- Phase 43: a landing after being airborne is queued for `take_landings`; hosts pass it to `ScriptRuntime::fire_landed` (fall damage, `Landed`). The first settle of Play isn't one.
- Seats (Phase 39) are `src/seat.rs`: `update_seats` runs at the start of `step` for every controller; a seated character is unanchored and welded to its seat (`keep_kinematic` skips it), and hosts fire `Seated` from `take_seat_changes`. Tests: `src/tests/seat.rs`. A VehicleSeat (Phase 40) also gets `Throttle`/`Steer` from the Move action and turns the camera with the car (`drive_seat`).
- Phase 46: controllers without input walk `MoveDirection` (`wants_to_move`, `move_character`); `PlatformStand`, `AutoRotate` (`turn`, from the first part's turn since first seen) and `Movement = DEAD` (not anchored) are the Humanoid's levers. `in_air` tells it Jumping from Freefall.
