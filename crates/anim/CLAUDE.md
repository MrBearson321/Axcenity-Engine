# `anim`

- One clip type (`files::ClipFile`, `*.anim.json`: tracks, events, an optional take `link`), state machines (`*.statemachine.json`), `Clip`, `Compiled`/`Machine` and `Animators`. Bone paths, `Bone` key numbers and Convert to keys: `bones.rs`. Inverse kinematics (posing joints, pins; plain math on a `Rig` and `Pose`): `ik.rs`. Key easings and Bezier handles: `easing.rs` (shared with tweens; every track samples through `sample_track`). Open one of: `docs/rendering/animation.md` (playback), `docs/scripting/api-animation.md` (Lua), `docs/project/animation-files.md` (files).
- Depends only on `core` and `assets`. Read files through `Assets`; skeletal poses from `Model::pose`.
- `Animators` is the only host: one runner per animated thing plays its Animator's state machine, then AnimationTracks and played Animations over it (latest start wins). A Ragdoll in a MeshPart gets a runner too (Phase 41, `ragdoll.rs`: Build, the humanoid preset, `pose_from_bodies`); its pose overrides the Animator's while it's on and blends back over `BlendTime`.
- Live state is `core::AnimatorState`, `AnimationTrackState` and `PropertyAnimationState` in the `World`, not saved properties.
- Format 1 files still read (`ClipFile::from_json` converts them). Keep example files canonical (`tests.rs` checks `to_json` round-trips them).
