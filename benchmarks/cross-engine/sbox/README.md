# XB10k in s&box

s&box 1.0.1 (sbox-dev on Paul's PC). `Code/XB10kSystem.cs` is a `GameObjectSystem`, so it runs in any game scene: when play starts it removes the scene's own objects (keeping its sky), builds everything from the formulas in [../README.md](../README.md), and times the frames. `Code/Behaviours.cs` holds the per-object components.

s&box needs the Steam client running. `sbox-cli` (in `<s&box>\bin\managed`, the engine's command-line bridge to a running editor or game) does the setup and the run:

1. Make the project from the Minimal template, and copy `Code/*.cs` into its `Code` folder: `sbox-cli --new "XB10k" --template game.minimal --in <folder>`. In `xb10k.sbproj` set `"TickRate": 60` (the template's 50 would step physics at 50 Hz).
2. Open it once in the editor so it compiles: `sbox-dev.exe -project <folder>\xb10k\xb10k.sbproj`. From the project folder, `sbox-cli --call editor_status` shows the compile state, and `sbox-cli --call play_start` presses Play (or press it by hand). Close the editor before timing.
3. Time it in the game client, not the editor: `sbox.exe -sw -w 1920 -h 1080 -mcp +game <folder>\xb10k\xb10k.sbproj` opens a 1920 × 1080 window and runs the project (`-mcp` lets `sbox-cli --connect <pid>` drive it). [`../run-all.ps1`](../run-all.ps1) does the rest for each run:
   - **Vsync** is read only at launch, from the player's own settings (`<s&box>\config\video.json`). The script writes `"vsync": false` there before launching and puts the file back afterwards. Setting `MenuUtility.RenderSettings.VSync` while the game runs doesn't take effect.
   - **The frame cap** (`fps_max`, 200 by default) isn't kept between launches, and game code may not run `fps_max`. So the script waits for the scene's first automatic run, sets `MenuUtility.RenderSettings.MaxFrameRate = 1000` through `sbox-cli --call run_csharp`, calls `XB10kSystem.Restart()`, and logs the second run with what `perf_overview` says limited it, which must be `CPU` or `GPU`, not `fps_max` or `vsync`.
   - The window's framing matches the spec (vertical field of view 70 from (−260, 180, −260)). `sbox-cli --call camera_screenshot` renders its own way and looks tighter, so don't judge framing by it.
4. The sun's shadow distance is the `r.shadows.csm.distance` console variable; the code sets it to 700 and logs `XB10K_SHADOW_DISTANCE`.
5. The console shows `XB10K_CHECK` (must be count=10000 sum=98894.50) and, a few seconds later, `XB10K_RESULT {json}` (`sbox-cli --connect <pid> --call read_console '{"filter":"XB10K"}'`); the JSON is also written to the game's data folder as `xb10k-result.json`.

Geometry is built in code (`Model.Builder`), one model per shape and size with its own collision; each object has a `ModelRenderer` (tinted) and a `ModelCollider`, and drops add a `Rigidbody`. s&box has no fixed time step per frame, so it runs in real time: at a few hundred frames a second, its 60 Hz physics steps only every few frames.

The code compiles against s&box 1.0.1's assemblies (checked with the .NET SDK, adding s&box's implicit `global using static Sandbox.Internal.GlobalGameNamespace;`), and runs in the s&box 26.11.10 editor and game client.
