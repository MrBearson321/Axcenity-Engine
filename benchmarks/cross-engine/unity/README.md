# XB10k in Unity

Unity 6.6 (6000.6.3f1 on Paul's PC), URP, Mono scripting backend (the IL2CPP Windows module isn't installed; add it in Unity Hub to compare). Everything is built at startup by `Assets/XB10k/Bootstrap.cs` from the formulas in [../README.md](../README.md); the scene file is empty.

1. Make a project: open this folder in Unity Hub (Add > from disk), or from the command line `Unity.exe -batchmode -quit -createProject <copy of this folder>`. `Packages/manifest.json` brings in URP.
2. Build: `Unity.exe -batchmode -quit -projectPath . -executeMethod BuildXB10k.Build -logFile build.log` (or the **XB10k > Build Windows Player** menu). It makes the URP asset if the project has none and writes `Builds/XB10k.exe`.
3. Run `Builds/XB10k.exe`. It sets 1920×1080 windowed, vsync off, no frame cap, and `Time.captureFramerate = 60`, so each frame is 1/60 s of game time. It logs `XB10K_CHECK` (must be count=10000 sum=98894.50) and `XB10K_RESULT {json}` to `Player.log` (`%USERPROFILE%\AppData\LocalLow\DefaultCompany\XB10k\Player.log`), writes `Builds/xb10k-result.json`, and quits.

Per-object scripts: each spinner has a kinematic `Rigidbody` and a `Spinner` MonoBehaviour; each blinker is a `Blinker` coroutine with `WaitForSeconds`, and its block has its own material; one `Lifter` coroutine lifts the drops. Cylinders and wedges collide as convex mesh colliders (Unity has no cylinder collider).
