# `anim`

- Take clips, property clips (`"kind": "property"`), controllers, `Animators` and `PropertyPlayers`; see `docs/rendering.md` (Animation) and `docs/scripting.md`.
- Depends only on `core` and `assets`. Read files through `Assets`; skeletal poses from `Model::pose`.
- Live state is `core::AnimatorState` / `PropertyAnimationState` in the `World`, not saved properties.
- Keep take-clip example files canonical (`tests.rs` checks `to_json` round-trips them).
