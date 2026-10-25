# `character`

- Kinematic character controllers. `ThirdPersonController` is an Instance in `core`; this crate steps it. It may query `physics`. `physics` must not depend on this crate.
- Behavior: `docs/scripting/api-character.md`. Frame order: `docs/architecture/frame-loop.md`.
- Hosts call `Controllers::step` after input and before scripts, then `keep_kinematic` immediately before the physics step. The first controller in the Workspace with a body receives movement and owns the camera.
- Movement is ray probes (`cast_ray_excluding`), not a physics body. Gravity is `physics::GRAVITY`; never change physics settings from here. Tests are in `src/tests.rs`.
