# `sim`

- The frame every host runs (roadmap M1): `Sim` is the simulation state, `Schedule` the data-driven step order, `HostIo` what differs per host (input, sound, network, drawing). Behaviour: `docs/architecture/frame-loop.md`.
- `sim` must not depend on `render`, `wgpu`, `egui`, `winit`, `kira` or `cpal`: the dedicated server links it. Check with `cargo tree -p axcenity-sim -e normal`.
- A step's place in `STANDARD` (`steps.rs`) is the player's order. A host that differs is listed in `DRIFT` with the reason; `golden.rs` fixes each host's order, so a change there is a deliberate behaviour change.
- Bench step names (`LAPS`) stay as `docs/benchmarks.md` lists them, so runs can be compared.
