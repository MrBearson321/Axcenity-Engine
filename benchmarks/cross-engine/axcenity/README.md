# XB10k in Axcenity

The project is generated, not checked in:

```text
python benchmarks/cross-engine/generate.py axcenity target/xb10k
cargo build --release -p axcenity-player -p axcenity-testing
target/release/axcenity-test target/xb10k                     # game test: scene and scripts work
set AXCENITY_VSYNC=off
set AXCENITY_WINDOW_SIZE=1920x1080
set AXCENITY_SAVE_DIR=%CD%\target\xb10k\save
start "" /affinity FFFF /wait /b target\release\axcenity-player.exe target\xb10k --bench 600
```

The player's `--bench` already runs 60 warm-up frames, then 600 at a fixed 1/60 s of game time, and prints one JSON line (`host`, `stages`, `startup`). `collect.py` turns that line into the common result format. `AXCENITY_VSYNC=off` stops the player waiting for the display (it uses vsync otherwise, which caps it at the monitor's refresh rate), and `AXCENITY_WINDOW_SIZE=1920x1080` sets the window in physical pixels. `check.server.lua` prints `XB10K_CHECK`; the test runner shows it (count=10000 sum=98894.50). `AXCENITY_SAVE_DIR` makes the player read the generated `save/XB10k/graphics.json` (Automatic quality, `shadow_distance` 700) instead of your own settings, so sun shadows reach the whole scene as the spec asks; `world.json` sets `Lighting.ShadowDistance` to 700 too. To see a frame from the benchmark camera, `shoot_project` (`docs/rendering/performance.md`) with `AXCENITY_SHOT_VIEWS=spec:-260,180,-260:0,0,0:70`, `AXCENITY_SHOT_SIZE=1920x1080` and `AXCENITY_SHOT_SHADOW_DISTANCE=700` (the preset's own distance would cap the world's 700); it draws the scene as built, before any frame runs.
