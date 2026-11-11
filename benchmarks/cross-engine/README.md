# XB10k: one scene, six engines

XB10k is the core of [S10k](../../docs/benchmarks.md#the-s10k-scene) rebuilt as nearly identically as each engine allows, so Axcenity can be timed against Godot, Unity, Unreal, s&box and Roblox on the same machine. It keeps what every engine has (parts, rigid-body physics, many small scripts, a sun with shadows) and leaves out S10k's sounds, animated characters and navigation agents, which differ too much between engines to compare.

Each engine builds the scene from the same formulas at startup (no hand-placed content), prints a checksum of what it built, and times the same window of frames. Results go in [results.md](results.md).

## The scene

Units are studs. Every engine uses 1 unit = 1 engine unit except Unreal (1 unit = 100 cm). Gravity is 196.2 units/s² downward everywhere (Roblox's default), so drops fall the same distance in the same time. Y is up.

| What | How many | Built from |
| --- | --- | --- |
| Floor | 1 static box | Size 440 × 1 × 440, centre (0, -0.5, 0), colour (0.35, 0.37, 0.40) |
| Grid blocks | 8,899 static | `i` in 0..8899 (below) |
| Spinners | 100 script-moved (kinematic) blocks | `i` in 8899..8999 (below) |
| Drops | 1,000 dynamic | `j` in 0..1000 (below) |

**Grid and spinners**, for `i` in 0..8999 (`SIDE` = 95, `SPACING` = 4):

- column `c = i % 95`, row `r = i / 95` (integer division)
- position `x = c·4 − 188`, `z = r·4 − 188`, `h = 1 + (i·7 % 5)`, centre `(x, h/2, z)`, size `(2, h, 2)`
- colour, with `hue = (i % 8)/8`: `(hue, 1 − hue, 0.5 + hue/2)`
- grid shape by `i % 3`: 0 block, 1 cylinder, 2 wedge. Spinners (`i ≥ 8899`, named `S0`…`S99`) are always blocks.
- grid blocks are named `G<i>`.

**Drops**, for `j` in 0..1000: `x = j % 10`, `y = j / 100`, `z = (j / 10) % 10`; centre `(x·3 − 13.5, 12 + y·3, z·3 − 13.5)`, size 1.5, colour (0.95, 0.6, 0.2), a ball when `j` is even and a block when odd. Named `D<j>`, kept in that order.

**Shapes** follow Roblox's rules so the geometry matches:

- block: a box of the given size.
- ball: a sphere whose diameter is the smallest size axis (1.5).
- cylinder: lies along X, length `size.x` (2), diameter `min(size.y, size.z)`.
- wedge: a box cut so the slope rises from the bottom front edge (−Z) to the top back edge (+Z). Cross-section in YZ is the triangle (y, z) = (−h/2, −1), (−h/2, +1), (+h/2, +1), extruded along X from −1 to +1. Collides as that shape (a convex hull).

All materials are plain lit, opaque, with the colour above, roughness about 0.5 and no metalness. Every object casts and receives shadows.

**Checksum.** After building, each engine prints `XB10K_CHECK` with the object count (10,000) and the sum of every object's centre x + y + z and size x + y + z, rounded to 0.01. The formulas give **count 10000, sum 98894.50** (see `generate.py`). A different sum means the scene is wrong.

## The scripts

Each behaviour is its own script instance or component, as an engine's users would write it (not one manager doing everything), because per-script overhead is part of what's being measured.

1. **100 spinners.** Spinner `k` turns at `30 + (k % 7)·20` degrees per second about Y: every frame `angle = (angle + speed·dt) % 360`, then set its rotation to `(0, angle, 0)`.
2. **97 blinkers.** Blinker `k` watches grid block `G<k·91>` and wakes every `0.1 + (k % 5)·0.1` seconds. Each wake: `count += 1`; read the block's position Y; if `count` is even and Y > 0, set its colour back to its starting colour, else set it to (1, 0.5, 0). Use the engine's own timer or wait, not a per-frame counter, where it has one.
3. **One drop lifter.** Every 1 second, for 100 drops in turn (a cursor `n` from 1 to 1000, wrapping): drop `D<n−1>` moves to `((n % 10)·3 − 13.5, 40, (floor(n/10) % 10)·3 − 13.5)` and its linear and angular velocity are set to zero.

## Camera, light and window

- Camera at (−260, 180, −260) looking at (0, 0, 0), vertical field of view 70°, near 0.1, far 2,000.
- One directional sun shining **from** the direction (0.5, 1, 0.25) (normalised), white, the engine's default intensity, shadows on at the engine's default shadow quality. Default sky and ambient.
- **Sun shadows reach 700 units from the camera**, so the whole scene casts them (the camera is about 400 units from its centre). Most engines' default shadow distance (Unity 50, Godot 100, Axcenity 150) would draw no shadows at all from this camera, which isn't the same work as an engine that draws them. Each engine's switch: Axcenity `Lighting.ShadowDistance` plus `shadow_distance` in the game's `graphics.json`, Godot `directional_shadow_max_distance`, Unity URP asset `shadowDistance`, Unreal `DynamicShadowDistanceMovableLight` = 70,000 cm for the simple profile (virtual shadow maps already reach), s&box the sun's shadow distance; Roblox has none to set. Cascade count and resolution stay at the engine's default.
- 1920 × 1080 window, **vsync off and no frame cap**.
- Each engine's default renderer settings for a new project otherwise. The run records which renderer and anti-aliasing it used. Unreal is run twice: defaults (Lumen, virtual shadow maps, TSR) and "simple" (Lumen off, shadow maps, no TSR), because its defaults do far more work than the others.

## Timing

- Build the scene, then let **60 warm-up frames** pass, then measure **600 frames**.
- **Fixed game time:** each frame advances exactly 1/60 s of game time, whatever the wall time, so every engine simulates the same 10 seconds. Physics steps at 60 Hz. Engines' switches for this: Axcenity `--bench`, Godot `--fixed-fps 60`, Unity `Time.captureFramerate = 60`, Unreal `-benchmark -fps=60`. s&box and Roblox have none; they run in real time and their physics may take fewer or more steps per frame, which the results note.
- **Frame time** is the wall-clock time between the starts of consecutive frames, from the engine's highest-resolution clock, collected in memory and summarised once at the end (no logging per frame).
- Report: median, mean, 95th percentile and worst frame (ms), frames per second from the median, scene build time, and the first frame after the build.
- **Output:** one line `XB10K_RESULT {json}` in the engine's log or console, with `engine`, `version`, `renderer`, `frames`, `median_ms`, `mean_ms`, `p95_ms`, `worst_ms`, `build_ms`, `first_frame_ms`, `fixed_timestep` (true/false). Where files are allowed, the same JSON is also written to `xb10k-result.json` next to the executable or project.
- **Runs:** release or shipping builds where the engine has them (Axcenity release, Godot export template release, Unity player build, Unreal Shipping or Development packaged game). Roblox and s&box run in their clients. Three runs each, pinned to the same CPU set (on Paul's 9950X3D, `start "" /affinity FFFF`), nothing else building or benchmarking, the median of the three medians reported.

## Known differences

These can't be made identical and are reported next to the numbers:

- Physics engines differ (rapier, Jolt, PhysX, Chaos, Box3D, Roblox's own), including sleeping rules, so the number of moving bodies at a time may differ.
- Roblox Studio's Play runs a server and a client in one process, and Roblox caps frame rate (set the cap to its highest, 240, in Studio settings); its numbers are an upper bound on cost.
- s&box and Roblox can't fix game time per frame.
- Renderers do different amounts of work by default (see Unreal above).

## Layout of this folder

- `generate.py`: the formulas, the checksum, and the Axcenity project and Roblox model writers.
- `axcenity/`: how to generate and run the Axcenity project.
- `collect.py`: turns logs from every engine into one results table.
- `godot/`, `unity/`, `unreal/`, `sbox/`, `roblox/`: each engine's project or scripts, and how to run it.
