# `audio`

- Plays project audio through Kira. Host behavior: `docs/architecture/play-mode.md`. Lua API: `docs/scripting/api-sound.md`.
- Hosts use `GameAudio`: it opens output lazily, applies the `SoundAction`s Lua queued, and syncs every Sound's properties each frame. The camera is the listener. `source_position` is where a spatial Sound is heard from. `AudioEngine` wraps Kira.
- Buses and effects (Phase 52): `mix` (it lives in `core` as `core::sound`, with `SoundAction`, so `script` and the server don't link Kira; re-exported here) reads groups, effects, ReverbZones and the Mixer's mute/solo from the world; `effects.rs` builds Kira effects. Group tracks are rebuilt when their shape changes, and Sounds move over. `render_tests.rs` hears the output through a capture backend; `AudioEngine` is generic over the backend for that.
- Read sound paths through `Assets::read_file`; never access project files directly.
