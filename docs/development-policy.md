# Development policy

The standing rules for how Axcenity is built, and the scope decisions behind them. Paul set them on 2026-11-10, in answer to the open questions in the [engine modernization report](research/engine-modernization-report.md#7-open-questions). Read this before you propose new infrastructure, a new platform target, or a roadmap item.

## The principle

Don't let future compatibility requirements dictate the engine's architecture before those capabilities are actually needed. Axcenity should become a reliable, capable desktop engine first. Supported platforms and deployment models can expand once the foundation is solid.

## The policy

1. Avoid introducing infrastructure before it is needed.
2. Prefer reliable, maintainable implementations over premature optimization.
3. Don't sacrifice core functionality for hypothetical future platforms.
4. Keep architectural extension points where reasonably inexpensive.
5. Require local tests for significant engine changes.
6. Avoid expanding features when existing implementations have unresolved correctness issues.
7. Revisit deferred capabilities at appropriate architectural milestones.

### What each rule means in practice

1. **Infrastructure.** No new CI jobs, build farms, hosted services, telemetry back ends or deployment tooling until a concrete need exists. A script under `scripts/` that a developer runs by hand is fine.
2. **Optimization.** A hotspot measured with the S10k benchmark or a profiler span ([benchmarks.md](benchmarks.md)) justifies an optimization. A guess doesn't. When a faster design is harder to keep correct, pick the simpler one until a measurement says otherwise.
3. **Platforms.** When desktop capability and a hypothetical mobile or browser target conflict, desktop wins. Examples: BC7 textures rather than a format chosen for phones, and Luau codegen even though it has no wasm path.
4. **Extension points.** Keep the cheap ones: deterministic iteration order where it costs nothing, a public scripting API that hides the internal representation, platform code behind one module. "Cheap" means it adds no second code path that has to be built and tested now.
5. **Local tests.** A significant change (to `core`, the frame schedule, physics, the scripting runtime, asset loading, rendering, networking, or anything a game can observe) adds or updates tests in the same change and runs them on a developer machine. The commands are in [development.md](development.md). A pull request says what ran and the result. There is no CI to catch what wasn't run.
6. **Correctness before features.** Fix a known correctness bug in a subsystem before adding features to that subsystem. Roadmap items that fix bugs (such as I2, Instances as Lua table keys) go ahead of new features in the same area.
7. **Revisiting.** Each deferred capability below names the milestone at which to look at it again. Until then, don't build toward it beyond rule 4.

## Decisions of 2026-11-10

### Continuous integration

GitHub Actions is **on for releases only** (decided 2026-11-11; on 2026-11-10 it was off entirely). Cost comes first: nothing runs per push or per pull request, and releases are not pushed often.

- `Release` (`.github/workflows/release.yml`) runs for a `v*` tag, or by hand, and ends in a draft release that a person publishes ([releases.md](project/releases.md#cutting-a-release)). **Tell Paul before anything sets a release run going**: which version, which platforms, and roughly how long.
- `CI` (`.github/workflows/ci.yml`) runs only by hand. Testing and verification happen locally during development: the commands in [development.md](development.md) and the platform check scripts (`scripts/check-platform.sh`, `scripts/check-platform.ps1`).
- Test a workflow change locally before pushing it: `python3 scripts/check-workflows.py`, and the scripts its steps call. Keep the toolchain pin in step with `rust-toolchain.toml`.
- "CI will catch it" is not verification. Say in the pull request what was run and on which OS.

### Deterministic simulation

Cross-machine deterministic simulation is **not a requirement**. The priority is reliable, consistent physics behavior and performance on a single machine.

- Lockstep multiplayer and deterministic replay are possible future capabilities. They must not add complexity now: no rapier `enhanced-determinism`, no fixed-point math, no cross-platform float control.
- Don't prevent them needlessly either. Keep deterministic ordering where it is already free: `BTreeMap`/`BTreeSet` orders, the fixed 60 Hz physics step, and ordered apply after parallel compute (report pattern 5.5).
- Replays in the plan (Phase 81) record the replication stream, so they don't depend on a deterministic simulation.
- **Revisit** when lockstep multiplayer or deterministic replay is actually planned.

### Platforms

The initial target is **desktop, with Windows and Linux as the primary platforms**.

- macOS is still a desktop platform the code builds for and the release packages ship, but it isn't a primary target, and changes aren't required to be verified on it.
- Mobile (Android, iOS) and web are **not immediate priorities**. Milestone 20 (Android and iOS) in `plans/axcenity-engine-unified.md` is deferred.
- Stay reasonably portable: avoid platform-specific dependencies where practical, and keep runtime file access going through the `assets` crate ([architecture/portability.md](architecture/portability.md)).
- Graphics formats and the scripting implementation favor desktop performance and capability. Don't give up a capability for hypothetical mobile or browser support.
- **Revisit** mobile and web once the desktop engine is reliable.

### Scripting API and Vector3

Axcenity provides a **Roblox-inspired** scripting API. It doesn't need perfect internal compatibility with Roblox, but commonly used Roblox-style APIs and the type behavior scripts expect are kept wherever reasonably practical.

- **Vector3 is a first-class engine type**: proper math operations, good performance, and clean interop with the rest of the API (CFrame, properties, attributes, RemoteEvent arguments, blocks).
- Exact `typeof(v) == "Vector3"` compatibility is desirable if it can be had without compromising the engine's architecture.
- **Caveat: don't choose Luau's native `vector` representation until its type identity and interop are verified** to give the scripting behavior above. Check at least: what `typeof` and `type` return; `.X`/`.Y`/`.Z` field casing; methods and properties (`Magnitude`, `Unit`, `:Dot`, `:Cross`, `:Lerp`) through the type metatable; equality and use as a table key; passing through RemoteEvents, attributes and saves; the luau-lsp definitions; and what the blocks translator emits.
- A compatible public API can sit over an optimized internal representation, so the representation can change later without breaking scripts.
- A starting point for the check: the sandbox's `typeof` is already the engine's own (`types::typeof_name`, [scripting/sandbox.md](scripting/sandbox.md)), so it could keep answering `"Vector3"` for a native vector. Luau's built-in `type()`, type annotations in luau-lsp, and anything that bypasses the sandbox would still see `"vector"`, which is part of what needs checking.

### Dedicated servers and hosting

Internet-hosted dedicated servers are a **future objective, not an immediate priority**.

- The focus now is reliable local multiplayer, the server/client architecture, networking fundamentals, and multiplayer testing in controlled environments: loopback, LAN, and a local `axcenity-server`.
- Deferred until the multiplayer systems are mature: dedicated-server deployment, hosting infrastructure, server management, and internet-facing deployment, including renet's secure mode and connect tokens (roadmap M11).
- **Revisit** once the underlying multiplayer systems are sufficiently mature.

## Deferred capabilities

| Capability | Status | Revisit when |
|---|---|---|
| GitHub Actions CI on pushes and pull requests | Off on purpose (CI runs by hand only); local testing instead | The architecture and core systems are stable |
| Cross-machine determinism (lockstep, deterministic replay) | Not a requirement | Lockstep or deterministic replay is actually planned |
| Mobile (Android, iOS) and web | Not a priority; Milestone 20 deferred | The desktop engine is reliable |
| Internet-hosted dedicated servers, secure netcode (M11), hosting and server management | Future objective | Local multiplayer and server/client fundamentals are mature |
| Luau native `vector` as Vector3's representation (half of L3) | Not chosen | The type identity and interop checks above pass |

## Roadmap items this changes

The item IDs are from the [engine modernization report](research/engine-modernization-report.md#6-refactoring-roadmap).

| Item | Change |
|---|---|
| I1 | Benchmarks run locally ([benchmarks.md](benchmarks.md)); no nightly CI run |
| M10 | The CI half (lavapipe GPU jobs, nightly benches, CI gates) is deferred. The local half stays: the platform check scripts already run `axcenity-test` and the headless server; the `intel_tex_2` player gate goes into those scripts when L2 adds the crate |
| M11 | Deferred with internet hosting |
| M12 | Gated on the GPU tests passing locally on a real GPU on Windows and Linux, before and after the upgrade, instead of on M10 |
| L2 | BC7 and KTX2 for desktop; ASTC and ETC2 aren't needed |
| L3 | `luau-jit` opt-in can go ahead. Native `vector` for Vector3 waits for the checks above |
| rapier `parallel` | Check results are consistent on one machine; cross-machine determinism isn't required |
