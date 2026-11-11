# XB10k in s&box

s&box 1.0.1 (sbox-dev on Paul's PC). `Code/XB10kSystem.cs` is a `GameObjectSystem`, so it runs in any game scene: when play starts it removes the scene's own objects (keeping its sky), builds everything from the formulas in [../README.md](../README.md), and times the frames. `Code/Behaviours.cs` holds the per-object components.

1. In sbox-dev, make a new game project (the Minimal template), and copy `Code/*.cs` into the project's `Code` folder.
2. Press Play in the editor, or export the game and run it standalone (preferred for timing). Set the window to 1920 × 1080 and turn vsync and any frame cap off in the game's settings.
3. The console shows `XB10K_CHECK` (must be count=10000 sum=98894.50) and, about 12 seconds later, `XB10K_RESULT {json}`; the JSON is also written to the game's data folder as `xb10k-result.json`.

Geometry is built in code (`Model.Builder`), one model per shape and size with its own collision; each object has a `ModelRenderer` (tinted) and a `ModelCollider`, and drops add a `Rigidbody`. s&box has no fixed time step per frame, so it runs in real time.

This was written without compiling against s&box's API; expect small fixes on first build.
