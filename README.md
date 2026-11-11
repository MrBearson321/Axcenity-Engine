# Toolbox inputs

Reference copies of source files from two of Paul's other projects, for the Axcenity Toolbox work in the engine (`docs/research/asset-sources/`). Not part of the engine and never merged; this branch only exists so cloud sessions can read them.

- `django-toolbox/`: from the Axcenity Toolbox Django site (CIDM 3312 Project 2), commit `5b17c1d`: `packs/manifest.py` and `tests/golden/pack_manifest.json` (the `axcenity-pack` v1 export).
- `3d-models/`: from the Blender asset pipeline, commit `6bf698d`: `tools/blender/providers/` (sources, registry, downloader, download guard, licence manager), `tools/library.py`, `docs/asset-metadata.md`, and the provider, library and record tests.

Checked before pushing: no `.env`, settings with keys, databases or credentials. The only token-like strings are a fake value in a test (`sekret-token-value`) and the name of an environment variable (`SKETCHFAB_API_TOKEN`).
