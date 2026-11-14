# Animation production (Priority 4)

Paul's 2026-11-14 list, Priority 4: Axcenity has animation authoring, skeletal meshes, GPU skinning, state machines, blend trees, root motion, IK and initial humanoid retargeting (G16 stage 1, #244). What's left is production quality, and the retargeting workflow comes first, before anything like motion matching.

## Where it starts (audit, 2026-11-14)

| Area | What exists | Gap |
| --- | --- | --- |
| Retargeting | `anim/src/retarget.rs`: 22 body roles by bone name, frame turn between Y-up and Z-up, hips scaled by leg length; `Clip::bind` retargets when names don't bind | No bone map you can edit, no reusable rig definitions, no fingers, no profiles, rest poses must match (T vs A pose plays as motion), feet slide when legs differ |
| Rig checks | Only "can't be retargeted because … (no LeftUpperArm, …)" when binding | No report before binding: hierarchy, swapped sides, rest pose, fingers, proportions |
| Editing | Animation tab: keys, curves, dope sheet, IK posing, Convert to keys (with key reduction) | No mirror, smoothing, loop seam fix, re-timing of baked keys |
| Sequencer | Shared transport (#159, #165): play/pause, stop, step, start/end, seek bar, preview rate, shortcuts | No loop toggle, play range, next/previous edit point, reverse play |
| Debugging | Animation tab preview draws joints; profiler shows animation time per thread | No skeleton overlay in the viewport or during Play, no live view of a running Animator (states, blend weights, layers, tracks, events) |

## Stages (one PR each, in merge order)

| ID | Stage | Content |
| --- | --- | --- |
| AP1 | Rig definitions, fingers, rig checks | `assets/rigs/*.rig.json` (role → bone name, reusable: applies to every skeleton that has all the bones it names); 30 finger roles matched by name (Mixamo, Unreal, Rigify, Unity/VRM, DAZ, Biped); `check_rig` / `check_pair` reports; `LoadedClip` carries the definitions to `Clip::bind_with` |
| AP2 | Retarget profiles | `*.retarget.json` picked by a clip's link (`"retarget"`): Match rest poses (T-pose take on an A-pose model), hips travel (scaled, unscaled, in place), plant feet and reach hands with IK, finger strength, per-role weight and turn offset |
| AP3 | Retarget panel | Animation tab: a model's bone map (role → bone, picked from its bones; save as a rig definition), the check report, a clip's profile settings, preview with the take's own skeleton beside it |
| AP4 | Cleanup and editing | Mirror (left ↔ right through the humanoid map), smooth, reduce keys on keyed clips, fix the loop seam, re-time a range |
| AP5 | Sequencer transport | Loop, play range (in and out marks), previous/next edit point (section ends, cuts, events), reverse play |
| AP6 | Debugging and visualization | Skeleton overlay in the viewport (edit and Play), a live Animator inspector (current state and blend, layers and weights, playing tracks, recent events, root motion) |
| AP7 | Motion matching | Proposal only, after retargeting is finished: [motion-matching.md](motion-matching.md) (decisions D1–D6, stages MM1–MM6) |

Decisions taken without asking (all reversible):

- Rig definitions are found by fit, not by a per-model setting, so one definition covers every character exported from the same rig and an animation-only file needs nothing extra. When several fit, the one naming the most bones wins.
- Profiles are skeleton-independent (settings by role), so a clip in `Shared/` with a profile still plays on any humanoid.
- Role names are Unity's and VRM's (`LeftIndexProximal`), the names most tools already export to.
