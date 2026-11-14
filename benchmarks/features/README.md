# Feature benchmark settings

Graphics settings files for `axcenity-player --graphics <file>`: each holds a benchmark run to these settings (as `graphics.json` holds them; a setting left out keeps the preset's), including whether the GPU is opened with hardware ray tracing, without touching the machine's own `graphics.json`. `benchmarks/baselines.json`'s showcase runs use them to draw the same scene and camera through each feature in turn. What the runs are and how to read them: `docs/benchmarks.md`, "Feature benchmark (showcase)".

| File | What it adds to Ultra |
|---|---|
| `ultra-rt-device.json` | The GPU opened with hardware ray tracing, no ray traced effect: what opening it costs on its own |
| `ultra-rt-shadows.json` | Ray traced sun and lamp shadows (and the fog's shafts), High (4 rays a pixel where soft) |
| `ultra-rt-ao.json` | Ray traced ambient occlusion, High (8 rays a pixel) |
| `ultra-rt-gi.json` | Ray traced global illumination, High (512 probes a frame, 256 rays each) |
| `ultra-rt-all.json` | All three at High, Ultra reflections (ray traced water reflections), volumetric fog and sun rays |
| `ultra-rt-all-upscaled.json` | The same at 67% render scale with TAA upscaling |

The presets alone (Low to Ultra) are run with `--quality`, not a file.
