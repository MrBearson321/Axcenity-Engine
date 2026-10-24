# `character`

- Kinematic character controllers. `ThirdPersonController` is an Instance in `core`; this crate steps it. It may query `physics`. `physics` must not depend on this crate.
- Behavior: `docs/scripting/api-character.md`. Frame order: `docs/architecture/frame-loop.md`.
- Hosts call `Controllers::step` after input and before scripts, then `keep_kinematic` immediately before the physics step. The first controller in the Workspace with a body receives movement and owns the camera.
