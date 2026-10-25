# `anim`

- One clip type (`files::ClipFile`, `*.anim.json`: tracks, events, an optional take `link`), state machines (`*.statemachine.json`), `Clip`, `Compiled`/`Machine` and `Animators`. Bone paths, `Bone` key numbers and Convert to keys: `bones.rs`. Open one of: `docs/rendering/animation.md` (playback), `docs/scripting/api-animation.md` (Lua), `docs/project/animation-files.md` (files).
- Depends only on `core` and `assets`. Read files through `Assets`; skeletal poses from `Model::pose`.
- `Animators` is the only host: one runner per animated thing plays its Animator's state machine, then AnimationTracks and played Animations over it (latest start wins).
- Live state is `core::AnimatorState`, `AnimationTrackState` and `PropertyAnimationState` in the `World`, not saved properties.
- Format 1 files still read (`ClipFile::from_json` converts them). Keep example files canonical (`tests.rs` checks `to_json` round-trips them).
