# `anim`

- Clip (`*.anim.json`) and controller (`*.controller.json`) files, state machines and blending, and `Animators`, which hosts own like physics; see `docs/rendering.md` (Animation) and `docs/scripting.md` (Animator).
- Depends only on `core` and `assets`. Read files through `Assets` (`read_file`, `takes`); poses come from `Model::pose`.
- Live animator state (parameters, `CurrentState`, `Play` requests) is `core::AnimatorState` in the `World`, never Instance properties, so it isn't saved.
- Keep the example files canonical (`tests.rs` checks `to_json` round-trips them).
