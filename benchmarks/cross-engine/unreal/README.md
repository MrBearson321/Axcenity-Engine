# XB10k in Unreal

Unreal Engine 5.8 (5.8.4 on Paul's PC), C++ project, no content assets: `AXB10kGameMode` (set as the global default game mode in `Config/DefaultEngine.ini`) builds everything when play starts, on the engine's empty `Entry` map, from the formulas in [../README.md](../README.md). Uses the engine's BasicShapes (cube, sphere, cylinder) and `BasicShapeMaterial`, and a `ProceduralMeshComponent` per wedge. One stud is 100 cm; spec Y-up becomes Unreal Z-up.

Needs MSVC (Visual Studio 2022 Build Tools with the C++ tools and a Windows SDK).

1. Build the editor target: `"C:\Program Files\Epic Games\UE_5.8\Engine\Build\BatchFiles\Build.bat" XB10kEditor Win64 Development -Project="<this folder>\XB10k.uproject" -WaitMutex`
2. Package a Development game: `RunUAT.bat BuildCookRun -project="<this folder>\XB10k.uproject" -platform=Win64 -clientconfig=Development -build -cook -stage -pak -archive -archivedirectory="<this folder>\Packaged"`
3. Run, defaults profile (Lumen, virtual shadow maps, TSR, as a new project from the Games templates):
   `XB10k.exe -windowed -ResX=1920 -ResY=1080 -benchmark -fps=60 -nosound`
4. Run, simple profile: add `-dpcvars=r.DynamicGlobalIlluminationMethod=0,r.ReflectionMethod=2,r.Shadow.Virtual.Enable=0,r.AntiAliasingMethod=2`

`-benchmark -fps=60` gives every frame 1/60 s of game time. The log (`Saved/Logs/XB10k.log`) has `XB10K_CHECK` (must be count=10000 sum=98894.50) and `XB10K_RESULT {json}`; the JSON also goes to `xb10k-result.json` in the project directory. The game quits itself after the run.
