# `audio`

- Plays project audio through Kira; see `docs/architecture.md`.
- `AudioEngine` owns output and sound handles; `SoundAction` is queued by Lua and applied by the host.
- Read sound paths through `Assets::read_file`; never access project files directly.
