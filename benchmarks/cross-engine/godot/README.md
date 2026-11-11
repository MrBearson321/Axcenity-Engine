# XB10k in Godot

Godot 4.7, Forward+ renderer, Jolt (the default since 4.6 for new projects; this project leaves physics on the default engine). Everything is built at startup by `main.gd` from the formulas in [../README.md](../README.md).

Import once, export a release build with the checked-in `Windows Desktop` preset (needs the 4.7.2 export templates, which the Steam build ships), then time the exported game:

```text
godot.exe --headless --path . --import
godot.exe --headless --path . --export-release "Windows Desktop" export/XB10k.exe
export\XB10k.exe --fixed-fps 60 --resolution 1920x1080
```

The editor binary runs it too (`godot.exe --path . --fixed-fps 60 --resolution 1920x1080`), but more slowly than the export.

`--fixed-fps 60` makes every frame advance 1/60 s of game time. The run prints `XB10K_CHECK` (must be count=10000 sum=98894.50) and `XB10K_RESULT {json}`, writes `xb10k-result.json` next to the executable (or to `user://`), and quits.

Per-object scripts: each spinner is an `AnimatableBody3D` with `spinner.gd`; each blinker is a `Timer` with `blinker.gd` and its block has its own material; one `Timer` with `lifter.gd` lifts the drops through `PhysicsServer3D`.
