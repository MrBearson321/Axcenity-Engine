# Default humanoid (Axcenity Humanoid character specification)

Paul's spec of 2026-11-14, "Axcenity Default Humanoid: Character Design and Technical Specification" (sections 1–14), with eight reference images of faceted low-poly characters. This file records what main had, the decisions, and the stages HD1–HD6, one PR each in merge order. The face (controls, profiles, expressions, lip sync, Character Studio's Face workspace) is the facial animation plan's: [facial-animation.md](facial-animation.md). The asset itself, how to rebuild it and its conventions: [assets/characters/humanoid/README.md](../../assets/characters/humanoid/README.md).

## Where it starts (audit, 2026-11-14)

| Spec area | Main before HD1 | Needed |
| --- | --- | --- |
| Default character | Six welded block Parts, 5 studs tall (`character/src/rig.rs`), a Humanoid, ThirdPersonController, Health, Inventory and an Animator with six empty states | A skinned, stylized person |
| Skeleton, retargeting | Humanoid roles by bone name (52, fingers since AP1), rig definitions, retarget profiles, `check_rig`/`check_pair` (AP1–AP3) | A skeleton those read with no fixes |
| Animation sources | FBX takes only; glTF animations weren't read | Blender's glTF export as a source |
| Blend shapes | Imported from glTF and FBX (facial F1) | Shapes on the default head |
| LODs | A file's `_LOD1`/`_LOD2` objects, paired with the full mesh by material; else meshopt at load | Spec asks LOD0–LOD3 |
| Customization | SurfaceAppearance per `MaterialName` with a `Color` tint; MeshPart object picks (`file.glb#A,B`) | Swappable parts, colours, body sliders |
| Controller | Kinematic, radius from the character's parts' extent, `MaxStepHeight` 1, `MaxSlopeAngle` | Capsule 0.30 m radius, step 0.35 m, slope 45°, independent of visuals |

## Decisions (taken without asking, all reversible)

- **D1 Style: faceted low-poly, after the reference images.** Flat shading, angular face, chunky brows, a wedge nose, simplified mouth, flat-coloured clothes. It sits between cartoon and realistic as the spec asks, and every reference image is in this style. The default is about 5,400 triangles (head 1,096, jacket 2,088, hands 640, the rest under 400 each), well under the spec's 25–35k: the faceted look doesn't gain from more. The budget is there for a smoother variant if Paul wants one.
- **D2 Built by a script, not modelled by hand.** `assets/characters/humanoid/build.py` makes everything in Blender (skeleton, meshes, materials, weights, face shapes, LODs) and `animations.py` keys the starter clips. Changes are reviewable diffs, the asset can be rebuilt anywhere (`bpy` from PyPI works without Blender), and it's original by construction: no copied character.
- **D3 Scale: authored at 1.75 m, placed at 5 studs.** Axcenity characters are 5 studs tall (the block rig, `Workspace.Gravity` 196.2 tuned for it), so 1 stud = 0.35 m for this character, and the spec's 0.35 m step is `MaxStepHeight` 1, its default. Import keeps "1 metre is 1 stud" for other models.
- **D4 Unity/VRM humanoid bone names.** `Hips`, `Spine`, `Chest`, `UpperChest`, `Neck`, `Head`, `LeftShoulder`, `LeftUpperArm`, `LeftIndexProximal`… are what `anim`'s retargeting reads as roles, so the skeleton maps all 52 roles with no rig definition, and Mixamo, Unreal, Rigify or Unity takes retarget onto it. Added: `Root` on the ground (root motion), `Jaw` and `LeftEye`/`RightEye` under `Head` (facial D4). Twist bones are left out for now (the engine skips them in role detection; LBS twisting at the wrist is mild at this style).
- **D5 Front is glTF's +Z; the default character turns 180°.** The engine's character controller faces -Z. The files keep the standard front so other tools read them right; the character's MeshPart sets `Orientation` Y 180.
- **D6 Relaxed A-pose at 45°.** The bind pose (spec section 2). Clips made in a T-pose retarget with "Match rest poses" (AP2).
- **D7 One default mesh, parts beside it.** `Humanoid.glb` is the default outfit as one skinned mesh (one MeshPart, one draw per material). `HumanoidParts.glb` has every part as its own object (base body, head, eyes, hands, jacket, shirt, trousers, shoes, four hair styles) on the same skeleton, with `_LOD1`/`_LOD2` each, for HD4's swappable parts. The single mesh has no file LODs because the engine pairs LOD copies with the full mesh by material, which parts share (`Skin`, `AccentCyan`); the engine's meshopt LODs serve it. LOD3 (the spec's 1–2k) needs a third engine level: HD6.
- **D8 Animations in one glTF file, in place.** `HumanoidAnimations.glb` holds the skeleton and 32 clips (HD1 reads glTF animations as takes). Locomotion is in place for the controller; `Walk_RootMotion` and `Run_RootMotion` move `Root` for root-motion use.

## Stages (one PR each, in merge order)

| ID | Stage | Content |
| --- | --- | --- |
| HD1 | glTF animations as takes | `model/gltf_takes.rs`: each glTF animation a take, its skeleton for retargeting (PR #368) |
| HD2 | The asset and an example | `assets/characters/humanoid` (generator, three GLBs, README, CC0), `examples/hello-humanoid` (Idle, Walk and Wave), tests that its clips play, its skeleton maps every role with no problems (`check_rig`, `check_pair` against all 32 takes, a 45° A-pose) and its head has the facial profile's 41 shapes; this file |
| HD3 | The default character | Insert ▸ Character (in a saved project) and new projects' StarterCharacter (Empty, Baseplate, Third Person, Vehicle, Arena) are the humanoid (`project::default_character`, `add_default_humanoid`): the model and animations embedded in `project` and written into the project where missing, with a clip per take, a state machine of 26 states and `assets/prefabs/Character.prefab.json`. The collision body is an invisible `HumanoidRootPart` (the 0.30 m capsule's box: 0.86 × 5 × 0.86 studs); the `Body` MeshPart is welded to it and doesn't collide. Controllers measure a character with a root part by it alone and crouch only it. The Humanoid plays Idle, Walk, Run, Jump, Fall, Crouch and CrouchWalk; the rest are for scripts. Not yet: the head hidden in first person (HD4, with parts), blend spaces and upper-body layers (the state machine supports both; clips need tuning), root-motion locomotion |
| HD4 | Modular parts and appearance | **First half (this PR):** skinned MeshParts inside a posed one follow its pose by bone name and are placed by its skeleton (`anim` followers), so a part swaps by `MeshId`; `project::add_humanoid_parts`, `humanoid_part`, `modular_character` (the base body as the leader, hidden while `Shirt`, `Trousers` and `Shoes` cover it, so it can't show through at the shoulders and waist). Colours already work per part through SurfaceAppearance tints by `MaterialName`. **Second half:** sockets as Attachments on bones (head, hands, back, hips, feet) for accessories, body sliders (height, shoulder, waist, hip, arm and leg length, head, hand and foot size) as bone scales, finer body masks per clothing piece |
| HD5 | Character Studio: Body and Outfit | Workspaces beside the facial thread's Face workspace: sliders, colour pickers, part pickers, sockets, a turntable preview, save as a prefab |
| HD6 | Pipeline and polish | An "Axcenity Humanoid" import preset (bone map, scale, front axis), import checks with repair guidance (missing roles, swapped sides, unit scale, bad weights, too many influences), a third LOD level in the engine, textures (2K default: soft gradients and AO baked into the flat look), optional ragdoll profile, Mixamo and Blender mapping profiles shipped as rig definitions |

## Speeds (HD3)

The engine's default `WalkSpeed` is 16 studs a second and `RunSpeed` 32: 5.6 and 11.2 m/s at this character's scale, a run and a sprint for a 1.75 m person. The default character keeps them (games are tuned for them) and plays its takes faster so the feet keep up: the Walk state is the Run take (3.6 m/s) at 1.56×, Run is Sprint (6.0 m/s) at 1.87×. Its controller's `CrouchSpeedScale` is 0.25 (4 studs a second, 1.4 m/s), so CrouchWalk (0.9 m/s) plays at 1.56× too. A game wanting a walking pace sets `WalkSpeed` near 5 and points the Walk state at the Walk take.

## Acceptance (spec section 14)

| # | Criterion | Where |
| --- | --- | --- |
| 1 | Readable, stylized, original, no logo | HD2 (D1, D2) |
| 2 | 1.75 m, 6.5 heads, A-pose | HD2 (tests: arm angle) |
| 3 | Imports and renders in Axcenity | HD1, HD2 (example renders) |
| 4 | Standard skeleton, retargetable | HD2 (tests: every role, no problems) |
| 5 | Facial shapes | HD2 (tests: 41 shapes); controls in facial F3 |
| 6 | Starter animations | HD2 (32 clips); controller in HD3 |
| 7 | Default player character, capsule, first and third person | HD3 |
| 8 | Modular parts and sockets | HD4 |
| 9 | Customization | HD4, HD5 |
| 10 | Within budgets, LODs | HD2 (D1, D7), LOD3 in HD6 |
| 11 | Blender pipeline and validation | HD2 (generator), HD6 |
| 12 | Documented | This file, the asset README, `docs/rendering/animation.md` |
