# Crates

Which crate owns what, and what it may depend on. Read this before adding a crate or a dependency.

| Crate | Owns | May depend on |
|---|---|---|
| `core` | Instance tree, arena, properties, events, services | nothing engine-internal |
| `assets` | the asset system: loading glTF/FBX models (with FBX skeletons, skin weights and takes), posing them, reading project files (`Assets`) and importing models | nothing engine-internal; plain data out |
| `anim` | animation: clip and animator controller files, state machines and blending, and `Animators`, which plays every Animator and poses its MeshPart | `assets`, `core` |
| `audio` | Kira playback for project sounds: `GameAudio` applies queued `SoundAction`s, keeps playback in line with each Sound, and spatializes emitters against the camera | `assets`, `core` |
| `blocks` | persisted block-script data and its one-way Lua translation | nothing engine-internal |
| `render` | `wgpu` renderer | nothing engine-internal; takes a draw list |
| `scene` | Instance tree → draw list (`build_draw_list`), the shared `FlyCamera`, viewport picking (`pick`) | `core`, `assets`, `render` |
| `physics` | `rapier3d` integration | `core`, `assets` |
| `script` | Lua 5.4 (`mlua`), sandbox, scheduler, engine API | `audio`, `core` |
| `ui` | UI Instance layout, CSS cascade (`taffy`) and screen-space draw data | `core`, `render` |
| `project` | project folder load and save, including CSS and block-script sources | `blocks`, `core` |
| `player` | runtime shipped with games (binary) | `anim`, `audio`, `core`, `assets`, `render`, `scene`, `physics`, `script`, `project`, `ui` |
| `editor` | `egui` editor app (binary) | `anim`, `audio`, `blocks`, `core`, `assets`, `physics`, `project`, `render`, `scene`, `script`, `ui` |

Block scripts are translated to Lua at project load and run through the same `ScriptRuntime` as handwritten scripts. Hosts hand queued sound actions to their `GameAudio` after script steps, and fire `Ended` for the sounds it reports finished; the player owns playback for its lifetime, while editor Play owns a separate `GameAudio` that is dropped on Stop.

Rules:
- `player` must **never** depend on `editor`, `egui` or `blocks`.
- `render` never sees Instances, scripts or the editor. Something outside it turns the world into a draw list (meshes, materials, transforms, lights, camera).
- `ui` reads the Instance tree and adds screen-space layout data to the render draw list; `render` remains Instance-free.
- Don't let crates depend on each other in a cycle. If two crates need each other, move the shared type into `core`.
- Pin dependency versions in the workspace's root `Cargo.toml` (`[workspace.dependencies]`).
