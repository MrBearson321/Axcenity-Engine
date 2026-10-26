# Frame order

The fixed order of one frame. Do not reorder it casually.

1. Read input. When a `ThirdPersonController` or `FirstPersonController` in the Workspace has a body, `character` steps it here (walk or run, jump and fall, crouch, slopes and steps, and its camera: following and wheel zoom, or at the eyes) before scripts run. Only the first such controller receives the keys.
2. Move tweens on (`TweenService`, setting their properties), then resume coroutines whose wait has expired, and fire queued events
3. Hold that character's parts kinematic (`Controllers::keep_kinematic`), then step physics at a fixed 60 Hz (accumulator; may run 0–N steps per frame). Other bodies keep the same gravity and friction.
4. Fire `Touched` and collision events, then `RunService.Heartbeat`; then particles and Animators move on (Animators pose their MeshParts and fire `StateChanged`)
5. Update UI layout (`taffy`) and shape text
6. Build the draw list and render

The player runs these in `WindowEvent::RedrawRequested` (`crates/player/src/main.rs`), with a comment per step. The editor runs steps 1-4 only during Play (`Session::frame` in `crates/editor/src/play.rs`); its viewport lays out UI at step 5 using its current pixel size, then builds and draws step 6. Visual-debug overlays (colliders, raycasts, bounds, emitters) are painted in the editor viewport after the scene texture; they do not change this order, and the player never draws them.

Since Phase 16, change events (`Changed`, `ChildAdded`, ...) for whatever changed fire at every step that runs scripts, before and after it, so physics' changes fire at the start of step 4 (see `docs/scripting/execution.md`).

Since Phase 14, step 1 also hands LocalScripts the camera (`set_camera`) and the key presses since the last frame (`ScriptRuntime::input`), and step 2 first delivers RemoteEvent messages to each side (server, then client) before resuming its tasks. Where a LocalScript moved the camera (`take_camera_move`) is applied just before step 6, unless a character controller owns the view; then that move is dropped and the controller's camera is drawn. See `docs/scripting/client-server.md` and `docs/scripting/api-character.md`.
