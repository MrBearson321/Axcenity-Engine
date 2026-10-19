# `audio`

- Plays project audio through Kira; see `docs/architecture.md`.
- Hosts use `GameAudio`: it opens output lazily, applies the `SoundAction`s Lua queued, and syncs every Sound's properties each frame. `AudioEngine` wraps Kira.
- Read sound paths through `Assets::read_file`; never access project files directly.
