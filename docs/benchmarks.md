# Benchmarks and profiling

How the whole engine frame is measured on every host, and the baseline later work is compared against (roadmap item **I1, "measure first"**, in the engine modernization report). Drawing has its own page: [rendering/performance.md](rendering/performance.md).

## The S10k scene

S10k is the standard measurement scene. `axcenity-test --make-s10k <folder>` writes it as an ordinary project (`testing/src/s10k.rs`), so every host loads it through its usual path. It is built the same way every time:

- **10,000 parts**: 9,000 anchored (a 440-stud floor, 8,899 grid blocks of three shapes and five heights, and 100 spinners) and 1,000 unanchored drops (balls and blocks) falling onto the middle of the grid. A script lifts 100 drops back up every second, so about the same number are always moving.
- **200 Scripts**: 100 turn a spinner every `Heartbeat`; 97 loop on `task.wait` (0.1 to 0.5 s), read a block's position and recolor it; `drops`, `sounds` and `agents` drive the rest.
- **50 looping Sounds** (hello-audio's chime), **20 animated characters** (hello-animation's Noodle, each with an Animator) along one edge, and **10 navigation agents** walking across the grid and back every 8 seconds.
- A SpawnLocation in a corner, so the player's and test runner's character stands clear of the grid.

## Running it

Build in release. Each host runs a 60-frame warm-up, then measures the frames asked for at a fixed 1/60 s of game time per frame, so every host does the same work per frame. It prints a table (mean, median, 95th percentile and worst time of each frame step) and one JSON line for tools.

```text
cargo run --release -p axcenity-testing -- --make-s10k target/s10k
cargo run --release -p axcenity-testing -- target/s10k --bench 600          # test runner (TestGame)
cargo run --release -p axcenity-server  -- target/s10k --port 0 --bench 600 # dedicated server
cargo run --release -p axcenity-player  -- target/s10k --bench 600          # player, with its window
AXCENITY_BENCH_PROJECT=target/s10k AXCENITY_BENCH_FRAMES=600 \
  cargo test -p axcenity-editor --release -- --ignored bench_project_play --nocapture --test-threads 1
```

`--bench` works on any project, not only S10k. After the table, each host also prints its **first frame** and its **worst warm-up frame** (where first-sight loads land), and the player adds **launch to first frame** and **launch to assets loaded** (from `main` starting to the end of the first frame drawn with the Workspace's assets in). The JSON line has these under `"startup"`. `core::bench` holds the shared pieces: `Laps` times a frame's steps one after another (the player's Developer Console Stats tab uses it too), `StageSamples` keeps every frame, and `BenchRun` adds the warm-up and the report.

### Frame steps

Each host names its steps the same way where they do the same work. A step a host doesn't have is left out.

| Step | What it covers | Hosts |
|---|---|---|
| Network | Network receive (and Play's network requests) | player, editor Play, server |
| Characters | Character controllers, seats, landings, Humanoids, players | player, editor Play, test runner (server: in Input) |
| Agents | `Agents::step`: paths, steering and navmesh rebuilds | player, editor Play, test runner |
| Input | Input events, prompts, tools, abilities | all |
| Scripts | `ScriptRuntime::step` (waits, tweens) and `Stepped` | all |
| Physics | Kinematic holds, shatters, `Physics::step` | all |
| Heartbeat | Touches, breaks, health, `Heartbeat` (and in the player, before its Animation step) | all |
| RenderStep | Render-step functions and the game camera | editor Play, test runner |
| Audio | Sound commands and syncing Sounds | editor Play (the player's is in Animation) |
| Animation | Animators, tools in hand, levels and loading screens (player: also audio and particles) | player, editor Play, test runner |
| Draw list | `build_draw_list` with UI layout | player |
| UI | Overlays (hotbar, chat, prompts, console) and network send | player |
| Render | `Renderer::render`, including waiting to present | player |
| Network send | Replication to clients | editor Play, server |

### Profiler spans

The `profiling` crate's spans mark the frame in every host and the hot functions: `build_draw_list`, `UiInteraction::layout`, `ScriptRuntime::step`, `stepped` and `heartbeat`, `Physics::step` with `Physics::sync`, `Physics::substep` and `Physics::write_back`, and `Replicator::frame`. They cost nothing unless a host is built with `--features profile-tracy`, which sends them to [Tracy](https://github.com/wolfpld/tracy) (connect the Tracy profiler while the game runs):

```text
cargo run --release -p axcenity-player --features profile-tracy -- target/s10k
```

The workspace hack leaves `profiling` out (`.config/hakari.toml`), so Tracy is never turned on by accident.

## Baseline

Taken on 2026-11-10 at the commit that added these benchmarks, in a Linux cloud container: 4 vCPUs of an Intel Xeon at 2.1 GHz, 15 GB of RAM, no GPU. The player ran under Xvfb on lavapipe (Mesa's CPU Vulkan driver), so its **Render** step is software rasterization sharing the same 4 cores, and its other steps are slowed by that contention. Treat the player's Render and frame numbers as "no GPU" numbers, not as a game's. Each host ran three times, 600 measured frames each; the table gives the median of the three runs' medians (run to run, steps over 1 ms agreed within about 10%; the widest were editor Play's Agents, 4.1 to 4.7 ms, and the player's Characters, 4.4 to 5.6 ms), and the worst frame seen in any run. Times are milliseconds.

| Step | Test runner | Server | Editor Play | Player |
|---|---|---|---|---|
| Network | – | 0.007 | 0.002 | 0.92 |
| Characters | 2.84 | – | 2.79 | 5.29 |
| Agents | 3.99 (worst 134) | – | 4.23 (worst 129) | 5.78 (worst 146) |
| Input | 1.82 | 0.075 | 1.79 | 1.88 |
| Scripts | 0.34 | 0.31 | 0.32 | 0.33 |
| Physics | 7.59 | 7.28 | 6.31 | 10.92 |
| Heartbeat | 1.52 | 1.37 | 0.88 | 1.58 |
| RenderStep | 0.011 | – | 0.008 | – |
| Audio | – | – | 0.086 | – |
| Animation | 0.28 | – | 0.26 | 0.56 |
| Draw list | – | – | – | 8.13 |
| UI | – | – | – | 0.087 |
| Render (lavapipe) | – | – | – | 61.0 |
| Network send | – | 0.65 | 0.004 | – |
| **Frame, median** | **18.6** | **9.8** | **16.8** | **97.8** |
| Frame, mean | 22.4 | 10.0 | 20.8 | 102.4 |
| Frame, 95th percentile | 27.0 | 12.4 | 23.0 | 118.1 |
| Frame, worst | 160 | 26 | 152 | 264 |

The server had no clients connected, so its Network send is the replication diff with nobody to send to.

### What the numbers say

- **Physics is the largest simulation step**: about 7 ms a frame for 1,000 moving bodies over 9,000 anchored ones, one 60 Hz step per frame. Roadmap M3 (physics sync overhaul) is measured against this.
- **The navmesh hitch is real (I7)**: Agents is 4 ms most frames, but its worst frame is 130–146 ms on every host that runs agents, against a 16.7 ms frame. The mean (7.5–9.4 ms) is nearly twice the median because of these rebuilds. The fingerprint hashes the moving drops, so they trigger rebuilds (fixed by I7, below).
- **Characters cost 2.8 ms with one player character**: the controller's ray probes against 10,000 parts.
- **Scripts are cheap at this load**: 200 scripts cost about 0.3 ms in step and 0.9–1.5 ms in Heartbeat (100 handlers writing `Orientation`). Their cost shows up elsewhere: the spinners' writes reach physics and the draw list.
- **The draw list is 8 ms** for S10k on the CPU, before any GPU work (roadmap M4, retained draw items).
- **Opening S10k is slow**: about 5.5 s in release for its 10 MB `world.json` (a 61-frame `--bench 1` run took 7.1 s in all), and about 30 s in a debug build. Loading is measured here only roughly; it isn't one of the frame steps.
- **A frame without drawing is 17–19 ms**, already over a 60 fps budget on this machine before rendering, and the server alone is about 10 ms.

### Loading ahead in the player (I11)

Same container, release build, 120 frames, three runs each. Milliseconds; ranges are the three runs.

| | hello-roads before | hello-roads after | hello-mesh before | hello-mesh after |
|---|---|---|---|---|
| First frame (= worst warm-up frame) | 2,069–5,431 | 341–398 | 1,226–1,500 | 139–166 |
| Launch to first frame | 10,989–14,532 | 8,934–9,541 | 9,502–10,054 | 8,812–9,165 |
| Frame, median | 14.2–15.1 | 15.2–16.4 | 41.2–41.6 | 40.9–42.5 |

hello-roads draws four 4K JPEG maps and a model; hello-mesh two FBX prop sets with their textures. Before I11 the first frame decoded every texture on the main thread. Now the preload thread starts right after the project is read, so the decodes overlap the window and GPU start-up (about 8.5 s here, mostly lavapipe) and are done before the first frame; the splash hides nothing half-drawn. What's left of the first frame is mostly the CPU mipmaps and upload of the 4K textures (roadmap L2 moves those to import time). Steady-state frames are unchanged within run-to-run noise.

### Navmesh rebuilds (I7)

Same container, release build, S10k, main and the I7 branch run back to back: three 600-frame runs of the test runner, two of editor Play, one 300-frame player run. Milliseconds; ranges are the runs.

| | Test runner before | after | Editor Play before | after | Player before | after |
|---|---|---|---|---|---|---|
| Agents, median | 4.2–4.6 | 0.36–0.38 | 4.2–4.5 | 1.5–1.6 | 6.5 | 0.70 |
| Agents, mean | 7.9–8.5 | 1.08–1.10 | 7.8–8.3 | 1.6 | 10.6 | 1.43 |
| Agents, worst | 120–137 | 26–29 | 123–127 | 4–14 | 141 | 24 |
| Frame, median | 19.4–21.2 | 14.6–14.7 | 16.6–17.4 | 13.3–13.7 | 111 | 102 |
| Frame, worst | 148–165 | 50–54 | 149–151 | 29–40 | 292 | 149 |

Before, the navmesh fingerprint hashed every part, the 1,000 falling drops included, and formatted values as text: about 4 ms every frame, and a 110–120 ms rebuild every 0.5 s. Now unanchored parts aren't ground, and a `MeshBuilder` reads again only the parts that changed. What's left of the worst Agents frame (about 20–30 ms in the test runner and player) is S10k's 100 anchored spinners: turning, their corners cross column centres, so the mesh really changes and is put together again every 0.5 s (gathering, linking and numbering 200,000 cells; roadmap M13 moves that to a worker). Editor Play rarely rebuilt (worst 4 and 14 ms); its 1.5 ms median is the check, every frame once 0.5 s has passed, that nothing the mesh is made from changed, which a world where nothing anchored moves also pays.

### Class and tag index (M2)

Taken on 2026-11-10 on a Ryzen 9 9950X3D under Windows, release build, main and the M2 branch run alternately. Each run was pinned to the V-cache CCD: unpinned, other work on the machine moved main's frame median between 12.7 and 19.2 ms and the comparison was noise. Medians of the runs' medians, in milliseconds; the frame's range across runs is in brackets. Test runner and server: seven 600-frame runs. Player: three 300-frame runs.

| Step | Test runner main | M2 | Server main | M2 | Player main | M2 |
|---|---|---|---|---|---|---|
| Characters | 1.147 | 1.049 | – | – | 1.379 | 1.272 |
| Agents | 0.118 | 0.065 | – | – | 0.144 | 0.086 |
| Input | 0.660 | 0.518 | 0.018 | 0.001 | 0.685 | 0.536 |
| Scripts | 0.121 | 0.010 | 0.117 | 0.006 | 0.129 | 0.011 |
| Physics | 3.911 | 3.652 | 3.559 | 3.387 | 3.994 | 3.783 |
| Heartbeat | 0.817 | 0.799 | 0.788 | 0.736 | 0.827 | 0.796 |
| Animation | 0.090 | 0.060 | – | – | 0.272 | 0.085 |
| Draw list | – | – | – | – | 5.563 | 5.454 |
| Network send | – | – | 0.297 | 0.270 | – | – |
| Render | – | – | – | – | 1.937 | 1.935 |
| **Frame, median** | **7.02** (6.52–7.62) | **6.23** (5.73–7.02) | **4.82** (4.72–5.39) | **4.44** (4.36–4.89) | **14.95** (14.77–15.79) | **14.03** (13.70–14.68) |

About 11%, 8% and 6% off the frame. Each walk replaced cost a few tens of microseconds on 10,000 parts, so the gain comes from there being many of them: `Scripts` (per-frame Health, Humanoid, follower and prompt walks) falls by about 0.11 ms on every host. A world with few Instances gains almost nothing. On copies of six real projects (General Store Roadside, Abandoned Farmhouse, three biome patches, The Dimming; under 1 MB of `world.json` each, on all three hosts) every difference was within run-to-run noise, in both directions.

### Calibrating against the rendering baseline

The render benchmarks ran on the same container to relate it to the machine in [rendering/performance.md](rendering/performance.md). `bench_10000_parts` (edge view) took 8.6 ms in `build_draw_list` here, against 3.3 ms there, so this container's CPU is roughly 2.5× slower for this work; divide the table above by about 2.5 for a fast desktop CPU. `bench_project` on S10k: `build_draw_list` 9.4 ms and `ScenePipeline::draw` (CPU side) 3.7 ms from the spawn, 9.5 ms and 3.7 ms from the overview, 6,575 and 10,026 items drawn. GPU times on lavapipe (300–940 ms a frame) say nothing about a real GPU and are left out.

Absolute numbers depend on the machine. Compare runs on one machine, and run the same commands before and after a change.
