# XB10k in s&box

s&box 1.0.1 (sbox-dev on Paul's PC). `Code/XB10kSystem.cs` is a `GameObjectSystem`, so it runs in any game scene: when play starts it removes the scene's own objects (keeping its sky), builds everything from the formulas in [../README.md](../README.md), and times the frames. `Code/Behaviours.cs` holds the per-object components.

s&box needs the Steam client running. `sbox-cli` (in `<s&box>\bin\managed`, the engine's command-line bridge to a running editor or game) does the setup and the run:

1. Make the project from the Minimal template, and copy `Code/*.cs` into its `Code` folder: `sbox-cli --new "XB10k" --template game.minimal --in <folder>`. In `xb10k.sbproj` set `"TickRate": 60` (the template's 50 would step physics at 50 Hz).
2. Open it: `sbox-dev.exe -project <folder>\xb10k\xb10k.sbproj`. From the project folder, `sbox-cli --call editor_status` shows the compile state, and `sbox-cli --call play_start` presses Play (or press it by hand).
3. Turn vsync and the frame cap off before timing: the engine's `RenderSettings.VSync` and `RenderSettings.MaxFrameRate` (Settings > Video in the game), or the `fps_max 0` console command for the cap. Use a 1920 × 1080 game view or window.
4. The console shows `XB10K_CHECK` (must be count=10000 sum=98894.50) and, about 12 seconds later, `XB10K_RESULT {json}` (`sbox-cli --call read_console '{"filter":"XB10K"}'`); the JSON is also written to the game's data folder as `xb10k-result.json`.

Geometry is built in code (`Model.Builder`), one model per shape and size with its own collision; each object has a `ModelRenderer` (tinted) and a `ModelCollider`, and drops add a `Rigidbody`. s&box has no fixed time step per frame, so it runs in real time.

The code compiles against s&box 1.0.1's assemblies (checked with the .NET SDK, adding s&box's implicit `global using static Sandbox.Internal.GlobalGameNamespace;`), but hasn't run in the engine yet.
