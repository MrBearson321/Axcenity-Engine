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
cargo run --release -p axcenity-server  -- target/s10k --bench 600 --bench-clients 8 # server with 8 players
cargo run --release -p axcenity-player  -- target/s10k --bench 600          # player, with its window
AXCENITY_BENCH_PROJECT=target/s10k AXCENITY_BENCH_FRAMES=600 \
  cargo test -p axcenity-editor --release -- --ignored bench_project_play --nocapture --test-threads 1
```

`--bench-clients <n>` makes the server host in memory and join `n` loopback players that read everything sent to them, as a player's device would, so the frame pays for replicating to them (roadmap M7). Measuring (and its warm-up) starts once every player has its snapshot. After the table it prints the snapshot's size, how many frames joining took, the bytes sent per frame and how long the players took to read them (not in the frame's steps).

**Frame rate and what limits it.** Under the table each host prints the frames' pacing (`core::bench::FramePacing`): average FPS (frames over the time they took), the 1% and 0.1% lows (the average frame rate of the slowest 1% and 0.1% of frames, at least one frame), the frame time's standard deviation and its 99th percentile. The JSON line has them under `"pacing"`. The player then prints the GPU's time for each pass from timestamps (`"gpu"` in the JSON; a frame the GPU timer skipped while its last reading was still on its way isn't counted, so it holds about half the frames) and a verdict, `"bound"` (`core::bench::Bound`): **GPU-bound** when the GPU's median is at least 85% of the frame's, **CPU-bound** when the CPU's work (the frame less the steps that wait, here Present wait) is, and **sync-bound** when neither is, which means waiting on the display or the two taking turns. Memory-bound isn't told apart: a texture budget that's full shows as its own note in the graphics settings, not in the frame.

**The player's own conditions.** `--bench` turns vertical sync off (`Renderer::set_vsync`) so frames are the engine's, not the display's, and times the GPU. `--size 2560x1440` makes the window that many physical pixels (it can be larger than the screen) and `--quality Ultra` draws with that preset alone for the run, leaving `graphics.json` as it is and ignoring a LocalScript's `UserGameSettings` changes. `Render` is the CPU's drawing work and `Present wait` the time the frame was blocked getting its picture from the window and handing it back (`Renderer::present_wait`).

**Where uneven frames come from.** `AXCENITY_BENCH_TIMELINE=<file.csv>` makes the player write every measured frame's steps there, the GPU's passes to `<file>.gpu.csv`, and each frame's camera, items drawn, triangles, draw calls and shadow casters to `<file>.counts.csv`. A frame-time spread with no matching change in counts is the machine; a step in the counts is the scene (Temperate Dry Patch's script moves the camera after 8 seconds, which first read as uneven frames).

`--bench` works on any project, not only S10k. After the table, each host also prints its **first frame** and its **worst warm-up frame** (where first-sight loads land), and the player adds **launch to first frame** and **launch to assets loaded** (from `main` starting to the end of the first frame drawn with the Workspace's assets in). The JSON line has these under `"startup"`. `core::bench` holds the shared pieces: `Laps` times a frame's steps one after another (the player's Developer Console Stats tab uses it too), `StageSamples` keeps every frame, and `BenchRun` adds the warm-up and the report.

### Regression check

`axcenity-bench-check` (`testing/src/bench_check.rs`) runs the scenes listed in `benchmarks/baselines.json` (S10k on every host, and copies of the owner's projects in the test runner and the player) and compares each with the numbers recorded for this machine. Run it before merging a change that could cost frame time, and after an optimization to record the new numbers:

```text
cargo build --release -p axcenity-player -p axcenity-server -p axcenity-testing -p axcenity-editor
start "" /affinity FFFF /wait /b target\release\axcenity-bench-check.exe --machine desktop-9950x3d   # Windows
taskset -c 0-15 target/release/axcenity-bench-check --machine <name>                               # Linux
```

- **Pin it.** Every scene it runs inherits the pinning. Unpinned, other work on the machine moves a frame by 30–50% and the check means nothing (`start /affinity FFFF` is the 9950X3D's V-cache CCD).
- **What it measures.** One uncounted warm-up round, then `--rounds` (3) counted ones; the median of the runs' frame medians, and for the player also its first frame and launch to assets in. A measure slower than its baseline by more than `tolerance` (10% for frames, 30% for startup times) prints `SLOWER` and the exit code is 1. A slower result is a reason to measure properly, alternating before and after builds, not proof on its own. Rerunning the recorded build straight after recording it, every measure came back within 6% (frames within 5%).
- **Projects.** A `project:<name>` scene is copied once from `AXCENITY_PROJECTS` (or `--projects`; default `Documents/Axcenity Projects`) into `target/bench-projects`, without its `.axcenity` folder, then cooked with `axcenity-editor --cook` before each run. The owner's project is never written. `--fresh` copies it again; a project that isn't on the machine is skipped.
- **Recording.** `--update` writes this run's numbers for `--machine` into `benchmarks/baselines.json` with the commit. Commit that file with the change that moved them, and say why in the commit. A run's `known` note (Abandoned Farmhouse's steady frame) is kept; edit it, and the machine's `about`, by hand when they go stale.
- **Per-run options and what else is noted.** A run's `args` are passed to the player (`dry-patch-ultra-1440p` draws Temperate Dry Patch at `--size 2560x1440 --quality Ultra`, the owner's desktop settings, for 400 frames: the project's script keeps its demo camera for 8 seconds, then hands it to the walker). A host that times the GPU also has its GPU frame median checked against `gpu_ms` with the frame's tolerance, and its 1% low frame rate and bound printed and recorded (`low_1pct_fps`, `bound`) but not judged: a 1% low is too few frames to fail a check on.
- `--only <text>` runs the scenes whose names contain it. Raw reports of the before/after comparison below are in `benchmarks/results/2026-11-10-desktop-9950x3d.jsonl`, one JSON line per run.

### Controlled scenarios

Each kind of load has a scene that isolates it, measured the same way every time (fixed cameras, size, preset and frame count). Most existed before; the regression check runs the ones marked "check" on every pinned run.

| Scenario | Where | How it's run |
|---|---|---|
| The owner's baseline: Temperate Dry Patch at Ultra 2560 × 1440 | `dry-patch-ultra-1440p` (check) | player `--bench`, demo camera |
| Lightweight, minimal geometry | `bench_20_animated_characters` (41 small items), hello-part | render bench, player `--bench` |
| Dense vegetation | Temperate Wet and Dry Patch (check), `bench_terrain_with_foliage` (20,000 plants) | player `--bench`, render bench |
| Several large trees | Abandoned Farmhouse (check), the patches' two 0.7–1 M triangle oaks | player `--bench`; `profile_project`'s `no-large-meshes` isolates them |
| High wind | any patch: `GlobalWind` is set; `profile_project`'s `no-wind` and `no-sway` measure it | `profile_project` |
| High shadow complexity | the patches at Ultra (four 4096² cascades, 40,000–64,000 casters); `no-shadows`, `no-foliage-shadows` | `profile_project` |
| High object count | S10k (check), `bench_10000_parts` | every host's `--bench` |
| High-poly realistic assets | `bench_2000_mesh_parts` (2,000 × 15,500-triangle helmets), General Store Roadside (check) | render bench, player `--bench` |
| Large terrain | General Store Roadside (960 × 1,260 studs, check), `bench_terrain_with_foliage` (2,048 studs) | player `--bench`, render bench |
| Particle-heavy | `bench_particles` (up to 100,000 GPU particles) | render bench |
| Animation-heavy | `bench_20_walking_characters`, S10k's 20 animated characters | render bench, `--bench` |

Render benches: `cargo test -p axcenity-player --release -- --ignored bench_ --nocapture --test-threads 1` ([rendering/performance.md](rendering/performance.md)). Results over time: `benchmarks/baselines.json` (the recorded numbers and commit per machine) and `benchmarks/results/*.jsonl` (raw reports).

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
| Render | `Renderer::render`: planning, uploading and recording the frame on the CPU | player |
| Present wait | Blocked getting the window's picture and presenting it (vertical sync, or the GPU frames behind); split off Render by `Laps::lap_with_wait` | player |
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

- **Physics is the largest simulation step**: about 7 ms a frame for 1,000 moving bodies over 9,000 anchored ones, one 60 Hz step per frame. Roadmap M3 (physics sync overhaul) is measured against this (results below: about 20% less).
- **The navmesh hitch is real (I7)**: Agents is 4 ms most frames, but its worst frame is 130–146 ms on every host that runs agents, against a 16.7 ms frame. The mean (7.5–9.4 ms) is nearly twice the median because of these rebuilds. The fingerprint hashes the moving drops, so they trigger rebuilds (fixed by I7, below).
- **Characters cost 2.8 ms with one player character**: the controller's ray probes against 10,000 parts.
- **Scripts are cheap at this load**: 200 scripts cost about 0.3 ms in step and 0.9–1.5 ms in Heartbeat (100 handlers writing `Orientation`). Their cost shows up elsewhere: the spinners' writes reach physics and the draw list.
- **The draw list is 8 ms** for S10k on the CPU, before any GPU work (roadmap M4, retained draw items).
- **Opening S10k was slow**: about 5.5 s in release for its 10 MB `world.json` (a 61-frame `--bench 1` run took 7.1 s in all), and about 30 s in a debug build. Almost all of it was one quadratic step, fixed in C11 (below): it now opens in 0.19 s in release and 0.8 s in debug. Loading is measured here only roughly; it isn't one of the frame steps.
- **A frame without drawing is 17–19 ms**, already over a 60 fps budget on this machine before rendering, and the server alone is about 10 ms.

### Loading ahead in the player (I11)

Same container, release build, 120 frames, three runs each. Milliseconds; ranges are the three runs.

| | hello-roads before | hello-roads after | hello-mesh before | hello-mesh after |
|---|---|---|---|---|
| First frame (= worst warm-up frame) | 2,069–5,431 | 341–398 | 1,226–1,500 | 139–166 |
| Launch to first frame | 10,989–14,532 | 8,934–9,541 | 9,502–10,054 | 8,812–9,165 |
| Frame, median | 14.2–15.1 | 15.2–16.4 | 41.2–41.6 | 40.9–42.5 |

hello-roads draws four 4K JPEG maps and a model; hello-mesh two FBX prop sets with their textures. Before I11 the first frame decoded every texture on the main thread. Now the preload thread starts right after the project is read, so the decodes overlap the window and GPU start-up (about 8.5 s here, mostly lavapipe) and are done before the first frame; the splash hides nothing half-drawn. What's left of the first frame is mostly the CPU mipmaps and upload of the 4K textures (roadmap L2 moves those to import time). Steady-state frames are unchanged within run-to-run noise.

### Loading off the lock (M5)

Real projects, on the owner's 32-thread Windows machine, release build, 120 frames, three alternating runs each, medians in milliseconds. Other builds were running on the machine, so a difference of a few percent means nothing.

| Project | Launch to assets in, main → M5 | First frame | Worst frame after warm-up |
|---|---|---|---|
| General Store Roadside | 4,104 → 3,438 | 1,515 → 1,215 | 16.3 → 13.6 |
| Abandoned Farmhouse | 5,495 → 5,860 | 1,043 → 2,891 | 12.0 → 11.5 |
| Arid Dry Patch | 5,056 → 3,204 | 2,941 → 1,017 | 168.1 → 59.2 |
| Temperate Wet Patch | 3,393 → 3,311 | 1,145 → 1,057 | 54.9 → 58.3 |
| Tropical Snow Patch | 3,208 → 3,061 | 716 → 765 | 16.8 → 17.1 |

Model-heavy projects gain most, because models used to parse one at a time. The player now reaches its first frame sooner, since startup no longer waits behind the global lock, and the first draw then waits for what's still loading: that's why Abandoned Farmhouse's first frame is longer while its total is about the same. The next section takes that wait out of the frame.

### Drawing without waiting for models (M5 follow-up)

Same machine and method, main (with M5) against the non-blocking draw list. Medians of three runs, milliseconds, with other builds running.

| Project | Launch to assets in, main → branch | First frame | Worst frame after warm-up |
|---|---|---|---|
| General Store Roadside | 3,811 → 3,776 | 1,341 → 1,347 | 11.2 → 10.7 |
| Abandoned Farmhouse | 5,443 → 6,812 | 2,303 → 3,951 | 9.1 → 11.5 |
| Arid Dry Patch | 5,392 → 3,650 | 1,012 → 824 | 64.6 → 62.1 |
| Temperate Wet Patch | 3,843 → 3,229 | 1,143 → 927 | 70.7 → 57.4 |
| Tropical Snow Patch | 2,985 → 3,010 | 675 → 682 | 14.4 → 14.1 |

Abandoned Farmhouse's first frame is texture work, not models. Timing its stages (three more runs, launch to assets in 5,257–5,563) gave Render 2,716–2,976, Draw list 48–52 and Physics 8–9. That Render time is the CPU mipmaps and upload of its textures, which land together on the first frame now that they're decoded by then; roadmap L2 moves them to import time.

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

### Navmesh on a worker, one mesh per game (M13)

The I7 leftover above, S10k's spinners making the mesh rebuild every 0.5 s, now runs on a worker thread: the game's thread reads the parts that changed (`MeshBuilder::look`) and a `navmesh build` thread puts the mesh together (`Assembly::run`), coming in 0.1 s of game time later (`nav::SharedNavMesh`, [navigation](architecture/navigation.md)). Agents and Humanoids' `MoveTo` now share that one mesh instead of building one each; S10k has no Humanoid walks, so the numbers below are the worker alone.

Test runner on S10k, the cloud container (4 vCPUs), `--bench 600`, main (320b2c5) and the branch run back to back, four runs each, milliseconds:

| | main | M13 |
|---|---|---|
| Agents, median | 0.21–0.29 | 0.26–0.31 |
| Agents, mean | 0.89–1.07 | 0.33–0.38 |
| Agents, worst | 25–36 | 4.0–5.6 |
| Frame, median | 11.1–12.9 | 11.4–11.8 |
| Frame, worst | 46–57 | 28–40 |

The worst Agents frame drops from a whole rebuild to the look that finds the change: 2.9–3.9 ms (timed by hand in one run), reading again the 100 turning spinners' solids and cutting them into columns, on the game's thread because it reads the world. Taking the finished mesh in, 0.1 s later, never showed among the slow frames: the worker had finished by then. The median is unchanged within noise; the worker shares the 4 cores with the rest of the frame while it runs. The frame's worst is now set by other steps (Physics, Characters).

### Class and tag index (M2)

Taken on 2026-11-10 on a Ryzen 9 9950X3D under Windows, release build, main (940fabc, with M1) and the M2 branch run alternately. Each run was pinned to the V-cache CCD: unpinned, other work on the machine moved main's frame median between 12.7 and 19.2 ms and the comparison was noise. Medians of the runs' medians, in milliseconds; the frame's range across runs is in brackets. Test runner and server: seven 600-frame runs. Player: three 300-frame runs.

| Step | Test runner main | M2 | Server main | M2 | Player main | M2 |
|---|---|---|---|---|---|---|
| Characters | 1.104 | 0.986 | – | – | 1.345 | 1.285 |
| Agents | 0.115 | 0.059 | – | – | 0.15 | 0.091 |
| Input | 0.639 | 0.501 | 0.018 | 0.001 | 0.681 | 0.534 |
| Scripts | 0.119 | 0.008 | 0.116 | 0.008 | 0.129 | 0.012 |
| Physics | 3.719 | 3.469 | 3.645 | 3.487 | 4.002 | 3.912 |
| Heartbeat | 0.81 | 0.763 | 0.788 | 0.759 | 0.836 | 0.793 |
| Animation | 0.087 | 0.057 | – | – | 0.275 | 0.092 |
| Draw list | – | – | – | – | 5.877 | 6.092 |
| Network send | – | – | 0.3 | 0.272 | – | – |
| Render | – | – | – | – | 1.98 | 2.028 |
| **Frame, median** | **6.68** (6.52–7.15) | **5.91** (5.78–6.25) | **4.88** (4.73–6.22) | **4.65** (4.50–5.65) | **15.48** (15.06–16.68) | **15.26** (14.09–15.67) |

About 12%, 5% and 1.5% off the frame (the player also draws, which M2 leaves alone). Each walk replaced cost a few tens of microseconds on 10,000 parts, so the gain comes from there being many of them: `Scripts` (per-frame Health, Humanoid, follower and prompt walks) falls by about 0.11 ms on every host, `Input` by 0.14 ms and `Animation` in the player by 0.18 ms. A world with few Instances gains almost nothing: on copies of six real projects (General Store Roadside, Abandoned Farmhouse, three biome patches, The Dimming; under 1 MB of `world.json` each, on all three hosts) every difference was within run-to-run noise, in both directions.

### Scheduler and handlers (M6, part 1)

The wait list became a heap, and handlers are dropped when their Instance is destroyed instead of every handler being checked each frame. Same cloud container, release build, main and the branch run back to back: three 600-frame runs of each host, S10k. Milliseconds, median of the runs' medians, ranges in brackets.

| Step | Test runner, main | Test runner, branch | Server, main | Server, branch |
|---|---|---|---|---|
| Scripts | 0.029 [0.029–0.031] | 0.018 [0.016–0.019] | 0.025 [0.025–0.026] | 0.017 [0.017–0.017] |
| Frame | 12.1 [11.9–12.3] | 12.4 [11.7–12.5] | 8.9 [8.9–9.3] | 8.7 [8.6–8.8] |

S10k has 200 scripts, so the Scripts step was already small; the frame difference is within run-to-run noise. The micro-benchmarks show where it matters, with many tasks or handlers (`cargo test -p axcenity-script --release --lib -- --ignored bench_scheduler bench_connections --nocapture`), nanoseconds per step:

| Case | main | branch |
|---|---|---|
| 10,000 tasks waiting, none due | 95,457 | 35 |
| 10,000 tasks looping on `task.wait` (about 1,000 resumed per step) | 1,364,000 | 1,364,000–1,424,000 |
| `ScriptRuntime::step` with 10,000 handlers, none destroyed | 59,363 | 2,949 |

With many tasks resuming every step, resuming them is the cost, and the queue doesn't change it.

### Property access and the watchdog (M6, part 2)

`cargo test -p axcenity-script --release --lib -- --ignored bench_member_access --nocapture` times one `instance.Key` read or write from Luau, by kind of member, less an empty loop. Wall-clock times on the cloud container are noisy (±20%), so the table also gives CPU instructions per access from Callgrind (`BENCH_ONCE=1 BENCH_ONLY=<case> BENCH_COUNT=<n>`, the difference between two counts), which don't vary between runs. main is b9fb148.

| Access | main ns | branch ns | main instructions | branch instructions |
|---|---|---|---|---|
| read `part.Position` | 512 | 397 | 6,580 | 5,417 |
| write `part.Position` | 383 | 393 | 5,213 | 4,904 |
| read `part.Anchored` | 356 | 241 | 4,696 | 3,537 |
| read `part.Name` | 394 | 282 | 5,151 | 3,992 |
| read `part.CFrame` | 637 | 479 | 7,558 | 6,411 |
| read `part.Touched` (event) | 741 | 473 | 8,173 | 6,010 |
| read `sound.Play` (class method) | 456 | 345 | 5,446 | 4,329 |
| read `workspace.P` (child) | 796 | 557 | 8,970 | 6,884 |
| call `part:IsA("BasePart")` | 266 | 192 | 3,713 | 2,844 |
| call an empty Lua function | 112 | 25 | 1,464 | 595 |

What the profile showed, and what changed:

- **The watchdog was the largest single cost.** Luau calls the interrupt at every function call and loop iteration, and each call looked up the `Watchdog` in mlua's app data twice (about 870 instructions) only to count. It now counts in a shared `Cell` and touches the app data once every 1,000 interrupts, or while something is stopped. This speeds up all Lua code, not only property access: an empty function call is about 4× cheaper.
- **Class methods** (`sound.Play`, `remote.FireServer`) are found in a table per class instead of a scan of every class's methods.
- **Keys stay `String`.** The roadmap suggested borrowing the key as an `mlua::LuaString`; measured, that is slower with mlua 0.12 (5,869 against 5,417 instructions for `part.Position`), because holding a Lua string from Rust pushes it to mlua's reference thread and pops it again, which costs more than copying a short key.
- **What's left is mostly mlua's.** Of the ~4,000 instructions in a `part.Name` read, the engine's own lookup (`instance::get`) is about 650, 400 of them making the result string. The rest is mlua: an Instance has both methods and `__index`, so mlua puts a generated Lua function in front of `__index` that checks the method table and makes an extra C call (`isfunction`) before calling Rust, and every Rust callback moves its arguments through mlua's reference thread. Removing that needs a change in mlua (a specialised `__index` wrapper) or dropping `add_method` for Instances, which would lose Luau's fast `__namecall` path for method calls. A `Vector3` result is a new userdata each read (about 980 instructions); that's roadmap L3 (native `vector`).

S10k doesn't move (Scripts 0.016–0.017 ms, frames within noise), as its 200 scripts do little per frame.

### Compiled-chunk cache (M6, part 3)

`cargo test -p axcenity-script --release --lib -- --ignored bench_script_start --nocapture` starts 200 copies of a 31 KB script (what cloning a model with scripts does). Same container, three runs each: main 2.40–2.51 ms per copy, the branch 0.18–0.28 ms, as only the first copy is parsed and compiled. S10k's 200 scripts are all different, so its load doesn't change.

### Reusing handler coroutines (M6, part 4)

`cargo test -p axcenity-script --release --lib -- --ignored bench_handler_fire --nocapture` fires `Heartbeat` 1,000 times with 1,000 handlers that don't wait. Against the branch before it (#27), Callgrind counts 6.83 billion instructions before and 5.58 billion after, about 1,250 fewer per handler call (−18%): making a coroutine and the garbage collection it caused were 40% of the run, and resetting a finished one is 8%. Wall-clock on the container is too noisy to show it reliably (665–755 against 613–724 ns per call); against main, with part 2's watchdog change, it's 1,050–1,150 against 600–680 ns.

### Physics sync (M3)

Taken on 2026-11-10 in the Linux cloud container (4 vCPUs, the same kind of machine as the baseline), release build: main (b9fb148, with M2 and M5), M3 with rapier's `parallel` feature off ("serial"), and M3 as merged ("parallel"), run in turn three times, 600 frames each. Medians of the runs' medians, in milliseconds. The container is noisy: run to run, Physics varied by up to 2 ms, so read the differences, not the absolute values.

| Step | Test runner main | serial | parallel | Server main | serial | parallel |
|---|---|---|---|---|---|---|
| Physics, median | 11.32 | 9.75 | **8.87** | 9.91 | 8.87 | **8.15** |
| Physics, 95th percentile | 17.71 | 15.21 | **12.57** | 15.72 | 13.56 | **11.38** |
| **Frame, median** | **19.57** | **17.79** | **16.81** | **13.21** | **12.15** | **11.69** |
| Frame, 95th percentile | 32.94 | 31.49 | 27.62 | 20.36 | 18.01 | 15.78 |

Physics fell by about 22% on the test runner and 18% on the server (29% and 28% at the 95th percentile), and the frame by 14% and 11%. Where the Physics step's time goes, from temporary timers around its parts on the test runner (about 9–11 ms in all here):

| Part of `Physics::step` | main | M3 serial | M3 parallel |
|---|---|---|---|
| Sync from the world | 1.6–1.7 | 0.6–0.8 | 0.7 |
| rapier's step | 6.0–7.2 | 5.4–6.0 | 4.8–4.9 |
| Write-back | 1.0 | 0.9–1.0 | 1.0 |

- **Sync** reads only what changed: physics no longer reads back what it wrote itself, and reparents read just the parts they touched. What's left is the 100 spinners a script turns every frame and the walk comparing each part's revision.
- **Write-back** is once per frame and looks only at awake bodies. It saves little on S10k because none of its 1,000 drops ever sleeps: they rest in one pile (one rapier island), and the drops script lifts 100 of them every second, which keeps the whole island awake. The ~1,100 parts written each frame cost about 0.8 µs each in `World::set_property` (string-keyed properties and the change log; roadmap L1 and M6). A world whose bodies come to rest now writes nothing. The benchmarks run one 60 Hz step per frame, so writing once per frame instead of once per step only pays when frames are slower than 60 Hz.
- **rapier's `parallel`** took about 1 ms (17%) off rapier's step on 4 cores, more in the 95th percentile, and should gain more on a desktop with more cores. In tiny scenes it costs a little: `hello-physics` 0.008 → 0.011 ms and `hello-vehicle` 0.05–0.07 → 0.19 ms a frame. A pile of 150 parts lands exactly the same way with and without it, with any number of threads.

The real projects (General Store Roadside, Abandoned Farmhouse, The Dimming, the biome patches) weren't run for M3: they're on the Windows desktop, not in the container.

### Physics step, part 1 (comparison C10)

The engine comparison (XB10k, PR #80) found physics the largest cost of an Axcenity frame. Timers around the parts of `Physics::step` on the test runner (cloud container, S10k, main `e52292c`) gave about 6.9 ms in all: rapier's step 4.0, finding touches 1.1, write-back 0.85, sync 0.5 and resetting forces 0.3. So about 40% was Axcenity's own work around rapier, all on one thread. What changed:

- **Forces** are reset only on the bodies the wind or a mover pushed in the last step (`Physics::pushed`), not by looking at every part's body (0.3 → 0 ms). Torques are reset too now: before, a Torque mover's torque, and the turn of an off-center VectorForce, grew every step (`a_torque_turns_as_hard_every_step_and_stops_when_disabled`).
- **Touches**: the step's contacts go into a sorted `Vec` merged with the sorted touching list (they were a `BTreeSet` and `BTreeMap`), and each collider's part and `CanTouch` sit in a table indexed by collider handle (`owners.rs`), so a contact needs no hash or part lookup (1.1 → 0.2 ms).
- **Write-back**: `World::set_known_property` takes a `&'static str` name and looks it up once, and `set_property` no longer allocates a list per change (0.64 → 0.54 ms).
- **Sync**: a part whose `revision_without_motion` didn't change since it was read whole has only its motion read again (`PartState::moved`), which S10k's 100 spinners hit every frame (0.39 → 0.29 ms).
- **Heartbeat**: the ~4,400 property changes physics writes a frame were each looked up as `Changed`, the property's signal and a Value object's value, on both sides; `changes::fire` now skips an Instance with no handler at all in one look (`Connections::has_owner`).

Main (`e52292c`) and the branch built into one target folder, copied out, and run alternately on the cloud container (4 vCPUs), four rounds of 600 frames each; medians of the runs' medians in milliseconds, ranges in brackets:

| Scene, host | Step | main | branch | change |
|---|---|---|---|---|
| S10k, test runner | Physics | 6.12 [5.74–6.12] | 4.73 [4.44–5.08] | −23% |
| | Heartbeat | 1.10 [1.05–1.11] | 0.54 [0.49–0.57] | −51% |
| | **Frame** | **8.07** [7.67–8.11] | **6.09** [5.68–6.52] | **−24%** |
| S10k, server | Physics | 6.04 [5.81–6.32] | 4.64 [4.01–4.72] | −23% |
| | Heartbeat | 1.09 [1.07–1.12] | 0.52 [0.49–0.53] | −52% |
| | **Frame** | **7.87** [7.60–8.22] | **5.84** [5.10–5.95] | **−26%** |
| XB10k, test runner | Physics | 6.06 [5.80–6.17] | 4.63 [4.60–4.70] | −24% |
| | Heartbeat | 1.08 [1.07–1.08] | 0.52 [0.51–0.53] | −52% |
| | **Frame** | **7.25** [6.97–7.36] | **5.24** [5.20–5.34] | **−28%** |
| XB10k, server | Physics | 6.07 [5.94–6.22] | 4.90 [4.75–4.99] | −19% |
| | **Frame** | **7.86** [7.69–8.00] | **6.13** [5.95–6.23] | **−22%** |

The 95th percentiles fell about as much (S10k test runner frame 10.18 → 8.11). The other steps didn't move. What's left of Physics is rapier's own step (about 3–4 ms here: the solver about 60% of it, contacts 30%), with about 0.95 ms of Axcenity's work around it: write-back 0.45–0.55 (string-keyed properties, roadmap L1), sync 0.3 (a revision check of every part, every frame) and touches 0.2.

### Physics step, part 2: physics' own threads (L4)

Part 1 measured on the owner's desktop (Ryzen 9 9950X3D, 16 cores and 32 threads, Windows 11), main `e52292c` against the branch, pinned to CPUs 0–15, three alternating rounds: S10k frame 3.71 → 2.47 ms (test runner) and 3.71 → 2.38 (server), XB10k 3.41 → 2.08; Physics about −30% and Heartbeat about −60%; `axcenity-bench-check` printed no `SLOWER` (S10k editor Play 3.32 → 2.11, player 7.24 → 6.07).

The same runs measured rapier's step on 1 to 16 rayon threads (`RAYON_NUM_THREADS`, S10k test runner, Physics median): pinned to 16 logical CPUs, 1 thread 2.5–2.7 ms, 4 threads 1.5–1.8, 8 threads 1.48 (best), 16 threads 1.6–1.8, and rayon's default 1.87. Unpinned, as a game runs: 8 threads 2.0, 12 threads 1.9, 16 threads 2.1, and the default 2.6–2.8. The default is 32 threads there: `std::thread::available_parallelism` reports every logical CPU on Windows, whatever the process's affinity, and one step's work spread over 32 threads costs more in waking and waiting than it gains. So rapier's step runs on physics' own rayon pool of at most 12 threads (`physics/src/threads.rs`, `AXCENITY_PHYSICS_THREADS` to choose; results are the same on any number, `a_pile_lands_the_same_way_on_any_number_of_threads`). The global pool, which the renderer and cooking use, is left as it was.

Same desktop, unpinned, three alternating rounds of 600 frames (medians of the runs' medians, milliseconds; "main" is `e3257295`, part 1 merged):

| Host, S10k | Step | main | own pool | change |
|---|---|---|---|---|
| Test runner | Physics | 2.71 [2.69–2.84] | 2.19 [2.17–2.32] | −19% |
| | **Frame** | **3.37** [3.35–3.49] | **2.81** [2.80–3.01] | **−17%** |
| | Frame, 95th percentile | 4.52 | 3.92 | −13% |
| Player (two rounds of 300) | Physics | 2.84–2.97 | 2.27–2.30 | −0.6 ms |
| | **Frame** | **8.79–8.83** | **8.36–8.37** | **−0.44 ms** |

Every own-pool run beat every main run. With `AXCENITY_PHYSICS_THREADS=8` the test runner's Physics was 2.09–2.12 ms (as good as 12, within noise) and with 16 it was 2.30. The player's Render step also gets faster (2.43 → 2.06 ms) when the global pool is capped too (`RAYON_NUM_THREADS=12`): the renderer's parallel work has the same oversubscription, left for its own change since it also moves cooking and the editor.

### Physics step, part 3: rapier's solver settings

Taken on 2026-11-11 in the Linux cloud container (4 vCPUs), release build. Times are S10k test-runner Physics medians over alternating runs of 600 frames, which swing by about ±20% here. Behavior is from a temporary test of 1×1×1 blocks on a floor at the default 196.2 gravity: stacks built six slightly different ways and stepped for 10 s, a chain of 12 ball sockets swinging down with a 1× or 8× end block, and a 300-block pile after 6 s.

| rapier setting (default) | Physics | What changed |
|---|---|---|
| 4 solver iterations, stabilization 1, recycle distance 0.05 | 4.5–5.8 ms | 2×2×2 block on 5 blocks stands in 2 of 6 builds, 10 blocks in 5 of 6; heavy chain's worst joint gap 1.4 m |
| 3 solver iterations | 4.2–4.5 | heavy chain gap 5.3 m; one terrain test fails |
| 2 solver iterations | 3.9 (about −20%) | 10 blocks fall in every build; heavy chain gap 7.2 m; one terrain test fails |
| no stabilization pass | 3.8–5.0 | about two thirds of the pile still jitters at 6 s; 4 tests fail |
| 6 solver iterations | within noise of 4 | heavy chain gap 0.7 m; stacks as with 4 |
| 8 solver iterations | not timed | heavy chain gap 0.4 m |
| **recycle distance 0.1** | same as 0.05 over 6 rounds | **both stacks stand in 6 of 6 builds**; all tests pass |
| recycle distance 0.2 | possibly faster, too noisy to say | a ball landing on edited terrain misses the edit |

Making rapier's step cheaper costs too much behavior, so the solver keeps its 4 iterations and its stabilization pass. The recycle distance is now 0.1 (`CONTACT_RECYCLE_DISTANCE` in `physics/src/lib.rs`, `a_heavy_block_on_a_short_stack_stays_up`): rapier keeps a touching pair's contact points, with the impulses carried over from the last step, until the pair has moved this far, rather than working them out again. Stacks of 15 blocks still fall at every setting tried.

### UI layout (M14)

S10k has no GUI, so the UI is timed on its own: `cargo test -p axcenity-ui --release --test bench_inventory_ui -- --ignored --nocapture` lays out a 500-element inventory (a ScreenGui with a titled panel and a ScrollingFrame of 10 rows of 10 slots, each an icon, a count, a name and a Use button; 499 elements shown) styled by a 16-rule stylesheet with custom properties, descendant selectors and `:hover`/`:active` rules, with the pointer moving over the slots. No GPU. Same cloud container, main (7f29f1a) and the branch built into one target folder and run alternately, three runs each; medians of 5 batches of 60 frames, milliseconds:

| | main | M14 |
|---|---|---|
| `layout_ui` (one pass) | 12.8–13.5 | 1.88–1.90 |
| `UiInteraction::layout` (a player frame) | 27.3–28.2 | 3.58–3.66 |
| Editor Play frame (hit-test layout, then `UiInteraction::layout`) | 53.4–55.8 | 3.60–3.69 |

What the profile (Callgrind) showed, and what changed:

- **The cascade was 88% of a frame.** Every element matched every rule against itself and each of its ancestors (for the custom properties it inherits), every time its styles were read: twice a pass for a container, as a child and as a parent. Each selector was also matched once per declaration of its rule rather than once per rule, and every match looked up `Interactable`, `Value` and the Tool's Inventory even for selectors with no `:disabled` or `:checked`. Now a `StyleCache` keeps each element's matched styles for the frame, an element inherits its parent's cached custom properties (shared, not copied, until it sets its own), a rule's selectors are matched once, and a state is looked up only when a selector asks for it.
- **A frame is two passes**: one with nothing hovered, to find what the pointer is over, then one with `:hover` and `:active`. They share the cache; the second rematches only the hovered and pressed elements and their subtrees. The first pass's boxes and text were painted and thrown away; now it isn't painted. Transitions ease an element once a frame however often its styles are read.
- **Editor Play laid the UI out four times a frame**: `layout_ui_interactive` with no pointer for its click hit tests (itself two identical passes), then `UiInteraction::layout`. `layout_ui_interactive` now stops after one pass when nothing is hovered or pressed, and the viewport calls `UiInteraction::hit_layout`, which `layout` reuses as its first pass, so editor Play costs what the player does.
- **Taffy was small** (about 5% of the frame after the rest): one `TaffyTree` per thread is cleared and reused for each container instead of a new one each time. Style lookups use Fx hashing, and border property names aren't formatted per element.

What's left is about evenly split between painting (text and boxes from CSS values, which are strings resolved on each read), Taffy, transitions and building the frame's styles. Nothing is kept between frames, so a retained layout (relayout only what changed, as M4 does for the draw list) is the next step if a game's UI needs it. The player's `UI` and `Draw list` steps on S10k don't change, as it has no GUI.

### Cooked textures (L2, first part)

Taken on 2026-11-10 in the Linux cloud container (4 vCPUs, lavapipe), release build, hello-roads (four 4096² JPEG maps on a SurfaceAppearance), after `axcenity-editor --cook examples/hello-roads` ([rendering/textures.md](rendering/textures.md)). Upload times are from temporary timers around `upload_texture`, on the player's first frame.

| | Before (decode, CPU mips, RGBA8) | Cooked (BC7) |
|---|---|---|
| Uploading one 4096² map on the first frame | 55–61 ms | 0.9–1.1 ms |
| Reading one 4096² map on an asset worker | about 650 ms (JPEG decode) | about 30 ms (file read) |
| GPU memory for one 4096² map with mips | 85 MiB | 21 MiB |
| First frame, three alternating runs | 436–475 ms | 441–488 ms |

- **The CPU's part of the first frame drops by about 230 ms** here (four maps), and by the same per map on any machine: no mipmaps to make and a quarter of the bytes to copy. Abandoned Farmhouse, whose 2.7–3 s first frame was this work for many embedded 4K maps (above), is the project to measure it on with a real GPU.
- **The first frame on lavapipe didn't change**, because lavapipe does the GPU's work on the CPU too: it decodes BC7 in software on every sample, which costs what the upload saved. The steady frame stayed within noise (Render median 17.8–21.5 ms both ways). On a real GPU BC7 sampling is free, so this needs the Windows desktop to show.
- **Cooking** took 32 s for hello-roads' 28 images (20 of them 4096²) on the container's 4 cores, once; 418 MB of cooked copies. Image quality with the "very fast" settings, PSNR against the source: 35.8 dB for the color map, 47.4 dB for roughness, 28.1 dB for the normal map ("basic" settings: 36.3, 47.7, 30.4). Normal maps would do better as BC5, which needs the shader to rebuild z; not done yet.

### Cached simpler copies (L2, offline LODs)

Taken on 2026-11-10 in the Linux cloud container, release build: `Assets::model_blocking` on a copy of each model in an empty project, first with no mesh cache, then twice with it ([rendering/models.md](rendering/models.md)).

| Model | First load (simplifies) | Later loads (cache) |
|---|---|---|
| CityPropsCollection.fbx (70.8k triangles) | 180 ms | 62–91 ms |
| DamagedHelmet.glb (15.5k triangles) | 476 ms | 446–451 ms |
| AWP.glb (39.8k triangles) | 1410 ms | 1195–1398 ms |

- **Simplifying was most of CityPropsCollection's fitting** (68 ms of it; 6 ms with the cache, hashing the primitives for the key included at 0.7 ms). What's left is ufbx parsing (36–42 ms) and reading its nodes (18–20 ms), which only a whole cooked model would remove.
- **The two glTF files are mostly texture decoding** (their embedded maps weren't cooked in this test; imported into a project they would be), so the cache saves only their simplifying, about 18 and 70 ms.

### Cooked models (L2)

Taken on 2026-11-10 in the Linux cloud container, release build: `Assets::model_blocking` in a project holding each model with its textures, cooked first (`cook_project`). The first load parses the file and keeps the cooked model; the next two read it ([rendering/models.md](rendering/models.md)). The glTF files were brought in with `import_model`, which copies their embedded maps out so they can be cooked.

| Model | Parse (textures cooked) | Cooked model |
|---|---|---|
| CityPropsCollection.fbx (70.8k triangles) | 143 ms | 1.7–1.9 ms |
| Walking.fbx (35.2k triangles, skinned) | 178 ms | 19–24 ms |
| DamagedHelmet.glb (5 embedded 2048² maps) | 71 ms | 8–13 ms |
| AWP.glb (50 MB, three 4096² maps) | 243 ms | 61–63 ms |

- What's left is reading the files: the cooked model, and each texture's cooked copy (a 4096² BC7 map with its mips is about 21 MiB). Without cooked textures, Walking.fbx took 683 ms to parse and wasn't kept (its embedded maps had no cooked copies yet).

### A Windows desktop with a real GPU

Taken on 2026-11-09 at `300bbb5` (I1 to I11 merged, so later than the table above): AMD Ryzen 9 9950X3D (16 cores), 62 GB of RAM, AMD Radeon RX 9070 XT on Vulkan, Windows 11. Each host ran three times, 600 measured frames each; the table gives the median of the three runs' medians, and the worst frame seen in any run. The first player run was slower than the other two (Physics 14.3 ms against 5.4 ms); it came straight after the release build. Times are milliseconds.

| Step | Test runner | Server | Editor Play | Player |
|---|---|---|---|---|
| Network | – | 0.008 | 0.002 | 0.009 |
| Characters | 2.70 | – | 2.13 | 2.41 |
| Agents | 0.32 (worst 13.6) | – | 1.17 (worst 2.7) | 0.35 (worst 12.6) |
| Input | 0.86 | 0.018 | 0.80 | 0.88 |
| Scripts | 0.12 | 0.12 | 0.12 | 0.14 |
| Physics | 4.94 | 3.85 | 3.76 | 5.46 |
| Heartbeat | 0.86 | 0.82 | 0.47 | 0.87 |
| RenderStep | 0.002 | – | 0.003 | – |
| Audio | – | – | 0.032 | – |
| Animation | 0.12 | – | 0.12 | 0.32 |
| Draw list | – | – | – | 8.38 |
| UI | – | – | – | 0.038 |
| Render | – | – | – | 2.69 |
| Network send | – | 0.52 | 0.003 | – |
| **Frame, median** | **10.0** | **5.4** | **8.7** | **21.5** |
| Frame, mean | 10.5 | 5.5 | 8.9 | 22.1 |
| Frame, 95th percentile | 13.3 | 6.9 | 10.9 | 25.0 |
| Frame, worst | 34 | 13 | 27 | 51 |

The player's first frame was 130 to 152 ms, and launch to first frame 3.3 s (7.0 s on the first run).

- **The draw list is now the player's largest step**, and at 8.4 ms it doesn't shrink on a faster CPU: it is 8.1 ms on the cloud container, where physics is about 1.7× slower. Retained draw items (M4) cut it to about a third on S10k: 12.9 to 4.3–4.4 ms median on the cloud container (see [rendering/performance.md](rendering/performance.md)).
- **Against the 3.3 ms in `rendering/performance.md`**: `bench_10000_parts` is 8.5 to 8.9 ms in `build_draw_list` here, so that figure is out of date. The roadmap's merges didn't cause it: 556b72b, the commit before them, is 8.0 to 8.8 ms on this machine, and each merge timed since is 7.4 to 8.9 ms. The rise came somewhere between the commit that added the render benchmarks and 556b72b.

#### Before and after the roadmap, same desktop (2026-11-10)

`300bbb5` against `5cb8302` (M1 to M7, M13, M14, L2 parts 1 to 7 merged). Release builds, each run pinned to the V-cache CCD (CPUs 0–15) with nothing else building, the two builds alternating, one uncounted warm-up round, then three. Medians of the runs' medians in milliseconds; the startup columns are the player's. The projects are copies of the owner's, cooked with `axcenity-editor --cook` for the after runs (the before build ignores cooked copies). Test runner and server ran 600 frames on S10k and 300 on the projects; the player 300.

| Host and scene | Frame before | Frame after | First frame before → after | Launch to assets in before → after |
|---|---|---|---|---|
| Test runner, S10k | 7.71 | 5.03 | | |
| Server, S10k | 5.13 | 3.82 | | |
| Player, S10k | 14.93 | 8.73 | 129 → 127 | 3,880 → 3,754 |
| Test runner, Abandoned Farmhouse | 0.53 | 0.25 | | |
| Player, Abandoned Farmhouse | 4.77 | **5.50** | **1,054 → 4,517** | **4,692 → 6,207** |
| Test runner, General Store Roadside | 1.81 | 0.094 | | |
| Player, General Store Roadside | 6.99 | 4.40 | 1,076 → 1,251 | 2,428 → 2,476 |
| Test runner, Temperate Wet Patch | 28.97 | 0.048 | | |
| Player, Temperate Wet Patch | 47.80 | 5.52 | 1,796 → 1,003 | 3,099 → 2,265 |

- **Abandoned Farmhouse's startup (was the open issue here) is fixed.** The table's after column is 5cb8302, when its startup was the M5 trade-off above: textures landed on the first frame, and `--cook` skipped the 4K maps its 29 `.glb` files embed (first frame 3.2–4.3 s uncooked, 4.0–4.2 s cooked). Three changes took that work out, each measured cooked on the same desktop (pinned, medians of 3):
  - **#65** cooks images inside models in place, once per project: its 144 embedded maps are 21 distinct images. First frame 3.0 s uncooked to 1.9 s cooked, GPU texture memory 947 to 311 MB, cook 6.8 s.
  - **#67** decodes TerrainLayer maps on the loading workers. A cooked texture decodes only when asked, and the terrain's arrays read 30 2K JPEG layer maps on the first frame (1.2 s). First frame 1.9 to 0.7 s.
  - **#69** resizes each terrain texture once and builds the arrays in parallel (`terrain.prepare` 526 to 79 ms). First frame 0.7 to 0.23 s.
  - Shots (`shoot_project`, three views) are pixel-identical before and after #67 and #69. Against uncooked, they differ only at single pixels on alpha-cut leaf edges (BC7 alpha near the cutoff).
  - The regression check at 15965da recorded first frame 212 ms and launch to assets in 1,579 ms, against 1,054 and 4,692 ms at 300bbb5 (`benchmarks/baselines.json`). General Store Roadside and Temperate Wet Patch also start faster: first frame 1,230 to 446 ms and 820 to 268 ms.
  - **Still open:** its steady frame is 5.4 ms, about 0.6 ms above 300bbb5's 4.77, not yet explained. The check's `known` note says so.
- **Editor Play and M1** (the possible 5.9% slowdown measured on the cloud container): `bench_project_play` on S10k, `4da1528` (before M1) against `1832da8` (M1's merge), seven alternating pinned rounds of 600 frames. The frame's median is 5.99 ms before and 5.09 ms after (ranges 5.05–6.48 and 4.57–5.22), Physics 3.34 → 2.29 ms, and the worst frame 13.8–17.2 → 5.9–6.9 ms. M1 made editor Play faster, not slower.
- **`bench_project` and `shoot_project` drew cooked textures uncooked** until their device asked for BC compression (`bench_gpu` in `player/src/bench_tests.rs`); a cooked hello-character now takes 0.2 s for its first 24 frames instead of 1.2 s. Numbers from those tools on cooked projects before that fix are the uncooked path's.

### The mouse's ray and mesh hulls (Input)

A 16-core Ryzen 9 9950X3D desktop, each run pinned to the V-cache CCD, release build, 300 frames, main and this change run alternately: three runs of the test runner, two of the player, medians of the runs' medians. Milliseconds.

| Scene | Test runner Input before | after | Frame before | after | Player Input before | after | Frame before | after |
|---|---|---|---|---|---|---|---|---|
| Arid Dry Patch | 30.9 | 0.006 | 31.4 | 0.21 | 36.8 | 0.013 | 44.9 | 6.7 |
| Temperate Wet Patch | 28.4 | 0.006 | 28.9 | 0.21 | 30.6 | 0.012 | 37.6 | 5.5 |
| Tropical Snow Patch | 1.06 | 0.006 | 1.29 | 0.21 | 1.21 | 0.013 | 7.8 | 6.3 |
| S10k | 0.50 | 0.016 | 5.9 | 5.4 | 0.52 | 0.023 | 13.7 | 13.2 |

`Clicks::step` casts the mouse's ray every frame, and a query builds a collider for each part near the ray. A MeshPart's collider was a new convex hull of every corner of its model, and the two scanned trees in those patches have 2-3 million corners between them (Tropical Snow's palms about 160,000). Hulls are now kept by model and size (`shapes::hull`), and the ray isn't cast at all when the game has no ClickDetector (`World::all_of`). The kept hulls also make every other ray, such as `workspace:Raycast`, cheap near heavy models. The editor Play bench keeps the camera at the origin, where its ray misses the trees, so it doesn't show the 30 ms (Input 0.020 before, 0.006 after).

### Replication to 8 players (M7)

The linux cloud container above, release build, `axcenity-server target/s10k --bench 600 --bench-clients 8`, main and this change run alternately, three runs each, the runs' medians (milliseconds) and their range.

| | main | M7 |
|---|---|---|
| Players who can join S10k | none: the server drops all 8 | all 8 |
| Snapshot | 14.4 MB (JSON) | 1.02 MB (postcard, only non-default properties) |
| Frames until all 8 have it | 242 (with the channel limit raised) | 19 |
| Network send, median | 45.2 (44.3–48.0) | 5.06 (4.86–5.30) |
| Frame, median | 57.4 (56.3–60.8) | 14.9 (14.1–15.2) |
| Bytes a frame to all 8 after joining | 10,324 | 5,464 |
| Players' reading, per frame | 0.19 | 0.055 |

- **On main nobody can join S10k.** Its snapshot is 14.4 MB of JSON, and the reliable channel holds 5 MiB (`transport.rs`), so renet drops each player as soon as the snapshot is queued ("The server closed the connection."). The main column was measured with that limit raised to 64 MiB in a local build, only to have numbers to compare. A Create now carries only what differs from its class's defaults, in postcard, so the same snapshot is 1.02 MB.
- **Network send was the motion sender.** Callgrind put 92% of main's network send with 8 players in `MotionSender::frame`: for each player, every frame, it read each of the 10,000 parts' Position, Orientation and velocity by name and rebuilt a set of all of them. It now re-reads a part only when its `motion_revision` moved, and keeps its parts in a list beside what the player has of each. What's left is mostly reading the ~1,100 parts that move, once per player, and `ScriptRuntime::owned_parts`.
- **The replication diff** skips a part whose only change was its motion (`revision_without_motion`) and compares what did change in place, so the 1,000 falling parts no longer each copy all their properties every frame.
- With no players, the server's frame is unchanged (Network send 0.65 vs 0.70, Physics 7.7 vs 7.7 over six alternating runs, within noise).

### Fast builtins and native code (L3)

`cargo test -p axcenity-script --release --lib -- --ignored bench_native --nocapture` starts one script per case and times it (best of five), with and without `--!native`. Cloud container, two runs, milliseconds; "main" is the same case before this change (no `safeenv`, see [scripting/sandbox.md](scripting/sandbox.md#fast-builtins-roadmap-l3)):

| Case | main | interpreted | `--!native` |
|---|---|---|---|
| numbers: 3M × `math.sin(i) * math.sqrt(i)` | 384–387 | 124–127 | 62–63 |
| fib(27), recursive | 26–27 | 28 | 22–24 |
| tables: fill and `ipairs` 300k | 18–20 | 12 | 9 |
| Vector3: 200k `v + Vector3.new(1, 0, 0) * 0.5` | 169–179 | 176–178 | 182–183 |
| Instance reads: 100k `p.Position.X` | 64–65 | 70 | 72–73 |

- **Most of the gain is `safeenv`, not native code.** Without it Luau treats every script's environment as possibly patched, so it looks each `math.sin` up and calls it through the C API, and native code bails out to the interpreter at each such call (that's why `--!native` alone measured 0.99× on "numbers"). With it, builtins run in place, and native code inlines them.
- **Engine calls don't speed up.** Vector3 math and Instance reads are mlua userdata calls either way (the Vector3 row allocates a userdata per result); native code can't inline them. That cost is what a native `vector` Vector3 would remove ([research/native-vector-check.md](research/native-vector-check.md)).
- **S10k doesn't move**: its 200 scripts mostly read and write Instances. Three alternating 600-frame runs each, medians: test runner frame 10.29 → 10.15 ms, Heartbeat 1.38 → 1.38; server frame 8.32 → 8.23, Heartbeat 1.33 → 1.36; Scripts 0.015–0.02 ms on both. `bench_member_access` moved 0–12% faster (a `part.Position` read 468 → 447 ns), within this container's noise.

### Query bounds (L4's first measurement)

L4 (parallel read, serial write) was to be "targeted by I1 data", so the data came first. On S10k's test runner the stages that could be split across threads are Characters (2.05 ms), Heartbeat (scripts, one Luau VM, 1.4 ms) and Physics (rapier, already `parallel` since M3). Callgrind on the Characters step (`--toggle-collect='*Controllers*step*'`, 20 frames) put **93% of it in `Reach::may_touch`**: every ray a character casts walked all 10,000 Workspace parts and read each one's `Position` and `Size` by name (string compares in a `BTreeMap`). Running that scan on more threads would have hidden it, not fixed it.

Queries now keep each part's bounding ball between rays (`physics::query::WorkspaceBounds`, [architecture/physics.md](architecture/physics.md)), checked by the Instance's revision. Same container, `main` and the branch alternating, three 600-frame runs each, medians in ms (ranges in brackets):

| | main | branch |
|---|---|---|
| Test runner Characters | 2.05 (2.05–2.06) | 0.31 (0.31–0.32) |
| Test runner frame | 10.03 (9.83–10.21) | 8.56 (8.48–8.90) |
| Server frame (no characters) | 8.12 (8.10–8.14) | 8.32 (7.81–9.18) |

Every query benefits the same way (`workspace:Raycast`, `Spherecast`, `GetPartBoundsInBox`, ...). After this, S10k has no stage left where splitting work across threads is the clear next step on a 4-core machine: Physics is rapier's own (already parallel), Heartbeat is one Luau VM, and the player's draw list became retained in M4. L4 stays open for an 8+ core measurement.

### Property slots (L1): measured, not now

L1 (a slot per property in each class's layout, `Prop` IDs instead of names, dirty bits) was to be built only if property access still dominated after M2 and M4. It doesn't.

Callgrind over whole S10k frames (test runner, after M2 and M4, 2026-11-10), share of the frame's instructions:

| | Share |
|---|---|
| Physics | 58% |
| Agents' navmesh refresh | 19% |
| `fire_changes` | 11% |
| Name lookups (string compares under `property` and `set_property`) | about 9% |

The other gains L1 promised, measured on 10,000 Parts in the Workspace (`World::new`, release, cloud container, 2026-11-11):

| | Today (`BTreeMap<&'static str, Value>`) |
|---|---|
| Properties per Part | 26 (`Value` is 48 bytes) |
| Heap per Part | about 3.3 KB in 8 allocations |
| `World::clone` (undo snapshots of some edits, Play) | 11.5 ms |
| `property(&str)`, 6 common names, all parts | 68 ns per read (mostly cache misses across 10k Instances) |
| `set_property("Position")` | 150 ns per write, change log included |

A per-class slot array would cut the heap to roughly 1.3 KB per Part and the clone to a few ms, but the frame cost is the name compares, and only `Prop` IDs at the ~500 call sites remove those. At about a tenth of the frame, and with no measured hitch from the clones (one per edit, not per frame), that isn't worth the change and its risk to the Properties panel, world files, saves and replication now. Revisit if a project shows `property`/`set_property` above about 20% of a frame, or `World::clone` becoming a visible edit hitch.

### Game files (M9)

The roadmap said to measure the ratio before compressing `.axgame` files. Each example was cooked (`cook_project`, as Build Game does), packed and unpacked on the cloud container (unpacking: best of three, page cache warm). Sizes in MB, times in ms:

| Game | Version 1 (main) | Version 2 (zstd per entry) | Unpack, main | Unpack, version 2 |
|---|---|---|---|---|
| hello-roads (four 4K maps) | 830 | 745 | 210 | 166 |
| hello-character | 454 | 387 | 135 | 98 |
| hello-events | 207 | 163 | 66 | 63 |
| hello-part | 109 | 87 | 34 | 31 |
| hello-mesh | 17 | 13 | 10 | 11 |
| S10k (no textures) | 10 | 0.2 | 30 | 41 |

- **Compressing is worth little for textures, a lot for worlds.** zstd over the whole file found 0.72–0.90 for cooked games (BC7 data) and 0.98–1.0 for uncooked ones (PNG and JPEG are compressed already), but `world.json` shrinks about 50×. Level 9 gave 0.81 where level 3 gave 0.83, at twice the time, so entries use level 3. Building takes longer: hello-roads packs in 3.3 s instead of 1.0 s.
- **Unpacking on several threads** makes up for the decompression; on one thread hello-roads took 520 ms.
- **The bigger size cost is shipping each texture twice**: a cooked game carries both the source image and its cooked copy (hello-roads: 393 MB before cooking, 830 MB after). Leaving out sources that have a current cooked copy would about halve such games, but the player still reads a source's pixels for some uses, so that's a separate change.
- **Reading in place** (no temporary copy) would need a file layer under the project loader, `assets`, `scene` and the player (about 600 file calls); with unpacking at 30–170 ms here it isn't the bottleneck.

### Collaborate on S10k (C11)

What a Collaborate session costs, measured on S10k (10,388 Instances, 200 scripts) on the cloud container (4 vCPUs), release build. All of these are ignored tests, run by hand:

```sh
cargo test -p axcenity-collab --release --lib perf_tests -- --ignored --nocapture
cargo test -p axcenity-editor --release --bin axcenity-editor idle_session -- --ignored --nocapture
```

**Joining.** The join message (the session's World) is 953 KiB in postcard. Messages of 16 KiB or more are now packed with zstd at level 1 when that saves at least an eighth (`wire.rs`, format 8): S10k's goes as 82 KiB (9%), packed in about 1 ms and unpacked in 0.6 ms, against 24 ms to encode and 43 ms to decode the message itself. Level 3 gave 99 KiB and level 9 70 KiB in 9 ms, so level 1 stays. The 200 scripts' 65 KiB pack to 1 KiB. Unpacking is bounded: a packed frame must state its size, and nothing over the receiver's limit (32 MiB to the host, 1 GiB to a member) is unpacked.

| Step, two editors over real sockets on this computer | Time |
|---|---|
| Start hosting S10k | 0.13–0.19 s |
| A member joins (connect, sign in, World, files caught up) | 0.30–0.38 s |
| The member opens the session's files as its project | 0.35 s (was 5.9 s) |
| Four members join in process, every message through `wire` | 0.49 s |

These are on one computer. Over Tailscale the member also waits for the bytes: about 82 KiB of World and the scripts (packed too, as file pieces), so a join of S10k should stay well under the 10 s goal on any link faster than about 1 Mbit/s; a two-machine run is still to be recorded. At 82 KiB, sending the World in pieces (snapshot streaming) isn't needed for S10k: over a 10 Mbit/s Tailscale link it's about 70 ms. It stays a C11 option for worlds an order of magnitude bigger.

**The member's open was the old world load.** A member writes the session's World to its folder and opens the folder as a project. That took 5.9 s, all in `parse_world`: each Instance looked up its prefab segment (`prefab::segment_of`), which lists every sibling, so the Workspace's 10,000 children cost 10,000² name comparisons. The segment is needed only inside a prefab copy, so it's now found only there. `project::load` of S10k went 5.66 → 0.19 s in release (`parse_world` 4.84 → 0.14 s; reading the JSON alone is 35 ms) and about 30 → 0.8 s in debug. Every editor and player opening a big project gains the same.

**Idle.** With nothing changing, `team_frame` on the hosting editor costs a median of 12–19 µs and a mean of 48–57 µs a frame; on the member a median of 2–3 µs and a mean of 34–36 µs (300 frames 16 ms apart). Both are under the 0.1 ms goal. The mean is mostly one frame in 31: every 0.5 s each editor's folder is compared with the session's files (`Mirror::sync`, for saves made outside the editor), about 0.4 ms in the collab crate (0.2 ms of it listing and reading the length and time of S10k's 209 synced files) and about 1 ms in the editor frame. One member run had a single 26 ms frame among those (others' worst was 1.1–1.4 ms); it didn't repeat. In process, without the editor, an idle host tick is 59 ns and an idle member's capture 37 ns.

**Presence and chat.** A busy member's presence (ten selected, the camera, a field being dragged, a caret in a script) is 125 bytes up and 126 down. Each editor sends at most 10 a second and only when it changed, the host drops one that didn't change, and each connection writes everything queued before one flush, so four busy members are about 19 KiB a second through the host (160 messages). Chat is held to 5 lines a second by the client and the host's budget. Nothing more to coalesce at this size.

### Calibrating against the rendering baseline

The render benchmarks ran on the same container to relate it to the machine in [rendering/performance.md](rendering/performance.md). `bench_10000_parts` (edge view) took 8.6 ms in `build_draw_list` there, about the same as the desktop above; steps other than the draw list are about 1.5 to 2× slower on the container. Compare runs on one machine rather than scaling between them. `bench_project` on S10k: `build_draw_list` 9.4 ms and `ScenePipeline::draw` (CPU side) 3.7 ms from the spawn, 9.5 ms and 3.7 ms from the overview, 6,575 and 10,026 items drawn. GPU times on lavapipe (300–940 ms a frame) say nothing about a real GPU and are left out.

Absolute numbers depend on the machine. Compare runs on one machine, and run the same commands before and after a change.

## Against other engines (XB10k)

`benchmarks/cross-engine/` holds XB10k: S10k's parts, physics and scripts (no sounds, characters or agents) rebuilt from the same formulas in Godot, Unity, Unreal, s&box and Roblox, with one timing rule and one result line for all of them. Its README is the spec; results go in `benchmarks/cross-engine/results.md`. For the player's part, `AXCENITY_VSYNC=off` turns vsync off (the player otherwise uses the surface's default, vsync, so `--bench` frames wait for the display) and `AXCENITY_WINDOW_SIZE=1920x1080` sets the window size in physical pixels.

Results on Paul's desktop (11 Nov 2026, median frame): Unity 3.11 ms, Axcenity 5.14 ms (6.70 ms before the physics PRs #84/#86/#93 and render pool #88), Godot 10.27 ms, s&box 2.05 ms (real time, not ranked); Unreal and Roblox not timed yet. What the numbers mean against each engine's features: [research/engine-comparison.md](research/engine-comparison.md).

## Editor graphics benchmark

Debug > Graphics Benchmark… in the editor measures the viewport on the machine it runs on and recommends the editor's own graphics settings (never a project's or a game's). It isn't a regression tool: its numbers are for the person at that computer. What it draws and how it decides: [rendering/settings.md](rendering/settings.md) ("Who changes them").

