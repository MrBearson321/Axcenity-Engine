# Motion matching (AP7 proposal)

Step AP7 of [animation-production.md](animation-production.md): a proposal only. Paul's Priority 4 says to finish the retargeting workflow before advanced animation technology such as motion matching. AP1 to AP3 finished it, and AP4 to AP6 added cleanup, Sequencer transport and debugging. This page says what motion matching would mean in Axcenity, what it needs first, and how it would be built in stages. Nothing here is built yet, and nothing should be until Paul says go.

## What motion matching is

Instead of hand-built states and transitions (Idle, Walk, Run, with blend trees between them), the animation system keeps a **database of poses**: every frame of a set of long, unstructured motion capture takes (a few minutes of walking, running, turning, starting and stopping). For each frame it stores a short **feature vector**:

- where the character will be, and which way it will face, a short time ahead (for example 0.33, 0.66 and 1.0 s), relative to its current position and facing;
- where its feet are and how fast they move;
- how fast its hips move.

Every few frames (about 10 times a second) the game builds the same kind of vector from what the character is doing now: its current pose's features, and the trajectory the player's input asks for. It searches the database for the frame whose features are closest (a weighted distance), and plays the take from there. A blend, preferably **inertialization**, hides the jump between the old frame and the new one.

The result is motion that responds to input at once and turns, starts and stops naturally, with no state machine to author. The cost is a large, well-captured dataset and some memory and search time per character.

Main sources: Simon Clavet, *Motion Matching and The Road to Next-Gen Animation* (GDC 2016, For Honor); David Bollo, *Inertialization: High-Performance Animation Transitions in Gears of War* (GDC 2018); Daniel Holden et al., *Learned Motion Matching* (SIGGRAPH 2020), with Holden's public articles and example code on features and inertialization. Unreal Engine 5 ships it as the Pose Search plugin with Motion Matching nodes, and Unity has open-source implementations (Kinematica was experimental and dropped).

## What Axcenity already has

| Need | Today |
| --- | --- |
| Playing frames of long takes on any humanoid | Linked FBX and glTF takes, humanoid retargeting with rig definitions, fingers and profiles (G16, AP1 to AP3) |
| More data from the same capture | Mirror left and right (AP4) |
| Root motion | Clips with `root_motion` move the MeshPart (Phase 39) |
| Feet that don't slide | FootPlacement and IKControl (Phase 79) |
| Running pose work off the main thread | Animators' pose jobs on the shared pool (unified Phase 93) |
| Seeing what the animation system decides | Animator Inspector and Skeletons overlay (AP6) |
| A character to drive it | Insert > Character: Humanoid, ThirdPersonController and an Animator with Idle, Walk, Run, Jump, Fall and Sit states |

What's missing: a pose database and its features, a trajectory from the character controller, the search, inertialization, and the tools to build and debug a database.

## Proposed design

Decisions to confirm with Paul (all reversible until built):

- **D1. A state kind, not a replacement.** Motion matching is a new kind of state in the existing state machine (beside a clip, a blend tree and a blend space). Locomotion becomes one Motion Matching state, and Jump, Fall, Sit and scripted actions stay authored states with normal transitions. Layers (an upper-body wave over locomotion), IK and ragdolls keep working unchanged, and games that don't use it pay nothing.
- **D2. A Motion Set file.** `*.motionset.json` lists the takes (linked clips, so retargeting applies), ranges to leave out, tags (such as `Idle`, `Strafe`, `Stop`), whether to add mirrored copies, and the feature weights. The feature database is built from it, at import or on first use, and cached under the project's cache folder like the model LOD cache, keyed by the files it was built from.
- **D3. Controller-driven movement.** The ThirdPersonController (or a script) still moves the character, so gameplay, physics and networking stay as they are. The animation follows the controller's trajectory, and a small correction nudges the root motion toward where the controller actually is. Whether the animation drives the capsule instead can be a later option per Motion Set.
- **D4. Inertialization for the jump between frames.** Each bone's offset from the old pose decays to zero with a critically damped spring over about 0.2 s, instead of crossfading two poses. It's cheaper than a crossfade (one pose sampled, not two) and keeps feet planted. It is useful for ordinary transitions too, so it comes first.
- **D5. Brute-force search first.** A flat array of normalised features, searched in full every 0.1 s, on the Animators' pose threads. A bounding-box tree over the features, as in Holden's articles, comes only when the profiler shows it's needed.
- **D6. No learned motion matching yet.** Learned motion matching swaps the database for small neural networks to save memory. It needs a training pipeline and an ML runtime that the engine doesn't have, so it stays out of scope until memory is a measured problem.

### Rough cost

Each frame's features are about 27 numbers: 3 future positions on the ground and 3 facings (12), 2 foot positions and velocities (12) and the hip velocity (3). Five minutes of capture at 30 fps, with mirrored copies, is 18,000 frames, so about 2 MB of features and 0.5 million multiply-adds per search. At 10 searches a second that's 5 million per character per second, small next to posing. Twenty characters fit in today's animation budget (S10k: 0.12 ms for 20 characters on the 9950X3D) with headroom, to be measured in MM3.

### Data

Motion matching is only as good as its data. The usual source is a dance-card capture: long takes of walking and running in every direction, with starts, stops, turns and pivots. Freely licensed sets are rare:

- Ubisoft's LaFAN1 is CC BY-NC-ND 4.0, so it can't ship in a game or as an example. It's fine for local research only.
- CMU's motion capture library is free to use but noisy and not a dance card.
- Commercial packs (such as Mixamo, or marketplace locomotion sets) have their own terms to check.

So the first deliverable needs either Paul's own capture or a pack he's licensed. The example project would use a tiny set made for it, which the engine can generate the way the noodle FBX is generated.

## Stages (one PR each)

| ID | Stage | Content | Test |
| --- | --- | --- | --- |
| MM1 | Inertialization | `anim` `inertialize.rs`: per-bone position and rotation offsets decayed by a critically damped spring; used for state machine transitions behind a per-transition `blend: "inertial"` option. Animator Inspector shows it. | A transition's pose is continuous at the switch and matches the target after the half-life |
| MM2 | Trajectory | The controller's predicted path: from Humanoid `MoveDirection` and WalkSpeed with a damped-spring prediction, or set by a script (`Animator:SetTrajectory`). Drawn by the Skeletons overlay during Play. | Prediction converges to a straight line at constant input, turns smoothly when input turns |
| MM3 | Motion Set and database | `*.motionset.json` (format in `docs/project/animation-files.md`), feature extraction from linked takes (retargeted, optionally mirrored), normalisation, the cache, and a bench of build and search times | Features of a generated walk take match hand-computed values; cache rebuilds only when a source changes |
| MM4 | Motion Matching state | The state kind: search every N frames on the pose threads, play from the best frame, inertialize the jump, root-motion correction toward the controller | A generated set of straight walks and 90° turns picks the turn when input turns; no foot slide beyond tolerance |
| MM5 | Tools | Animation tab: a Motion Set editor (takes, ranges, tags, weights, Build) and a database view; the Animator Inspector gets the current take and frame, the search's cost broken down by feature, and the desired against matched trajectory in the viewport | Editor tests as for the retarget panel |
| MM6 | Tags and tuning | Tag filters from parameters (such as `Crouched` picking `Crouch` frames), an optional feature tree for big sets, and tuning presets | Tag filter never picks an untagged frame; tree search equals brute force |

MM1 and MM2 are useful without the rest, so they're the low-risk start. MM3 onward waits for a dataset.

## Risks and open questions

- **Data and licensing** (above): the main blocker.
- **Memory per character:** the database is shared between characters that use the same Motion Set, so it's per set, not per character.
- **Networking:** poses aren't sent over the network today: replication sends each Animator's parameters and layer weights (`net` `WireAnimator`), and each device poses characters itself. Motion matching would run the same way, from the replicated movement and parameters, so no protocol change, but two devices may pick slightly different frames. That's acceptable for animation, and matches Paul's decision not to chase cross-machine determinism now.
- **Authoring cost:** small teams may prefer the existing state machine with blend spaces. Motion matching stays optional (D1).

## Recommendation

Build MM1 (inertialization) and MM2 (trajectory) when animation work is next scheduled, since they improve every state machine. Start MM3 only once there's a licensed locomotion dataset to test against.
