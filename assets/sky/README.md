# Default sky

`partly-cloudy-sky.hdr` is *Kloofendal 48d Partly Cloudy (Pure Sky)* by Greg Zaal and Jarod Guest, from Poly Haven (https://polyhaven.com/a/kloofendal_48d_partly_cloudy_puresky), at 2K (2048 × 1024, Radiance `.hdr`, unchanged). Poly Haven releases it under CC0 1.0 Universal: see `LICENSE.txt`. Credit is not required, but appreciated.

The `project` crate embeds it (`DEFAULT_SKY`, `crates/project/src/default_sky.rs`): New Project's Baseplate and Empty templates write it to `assets/sky/partly-cloudy-sky.hdr` and set it as Lighting's `SkyImage`, with `ClockTime` and `GeographicLatitude` placing the sun where the picture's sun is. Lighting's `SkyImage` row in Properties puts it in any project with its sun button. See `docs/rendering/sky.md`.

Its sun is 47.9° up, at pixel (1218, 239). Why 2K: 4K is 20 MB and only sharper where the sky fills the screen; 1K blurs the clouds.
