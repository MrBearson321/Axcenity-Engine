# Blender pipeline: ambientCG and Poly Haven through the Toolbox

Drop-in files for the 3D Models toolkit (`tools/blender/providers/`), so
its ambientCG and Poly Haven providers search and download through the
engine's Toolbox (the `axcenity-toolbox` command and the `axcenity_toolbox`
package one folder up) instead of their own Python code. Downloads then land
once in the central asset directory, verified, with their source and licence
in `metadata.db`, shared with the editor's Toolbox tab.

1. Copy `providers/toolbox_backed.py` to `tools/blender/providers/` and
   `tests/test_toolbox_providers.py` to `tests/`.
2. Apply `providers-registry.patch` from the toolkit's root
   (`git apply path/to/providers-registry.patch`): the registry wraps
   `AmbientCG` and `PolyHaven` with `backed(...)`.
3. Point the toolkit at the engine: `AXCENITY_TOOLBOX` to the built
   `axcenity-toolbox` binary (`cargo build -p axcenity-toolbox --release`)
   and `AXCENITY_TOOLBOX_PYTHON` to this repo's `crates/toolbox/python`.
4. Run the provider tests: `python -m unittest tests.test_toolbox_providers`
   offline, then `PIPELINE_LIVE_TESTS=1 python -m unittest tests.test_providers`
   for the real searches and downloads (they go through the backed classes).

Without the binary, or with `PIPELINE_TOOLBOX=0`, each provider keeps its
own Python code. That code is deleted only after step 4 passes on the
machine that runs the pipeline (the conditional migration in
`docs/research/asset-sources/`). Old downloads move with
`axcenity-toolbox migrate <asset_library>` (only files that verify).
