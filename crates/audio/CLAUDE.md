# `audio`

- Plays project audio through Kira. Host behavior: `docs/architecture/play-mode.md`. Lua API: `docs/scripting/api-sound.md`.
- Hosts use `GameAudio`: it opens output lazily, applies the `SoundAction`s Lua queued, and syncs every Sound's properties each frame. The camera is the listener. `AudioEngine` wraps Kira.
- Read sound paths through `Assets::read_file`; never access project files directly.
