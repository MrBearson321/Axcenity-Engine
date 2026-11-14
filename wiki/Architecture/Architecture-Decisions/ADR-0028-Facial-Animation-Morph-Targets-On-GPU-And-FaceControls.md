---
title: "ADR-0028: Facial animation: GPU morph targets, FaceControls and face profiles"
type: adr
adr: 28
adr_status: Accepted
date: 2026-11-14
supersedes:
superseded_by:
affects: [assets, anim, render, scene, core, script, net, editor]
source: [crates/assets/src/model.rs, crates/render/src/renderer/skinning.rs, crates/render/src/shader.wgsl]
docs: [docs/research/facial-animation.md]
tags: [adr, animation, rendering, characters]
---

# ADR-0028: Facial animation: GPU morph targets, FaceControls and face profiles

- **Status:** Accepted (Paul's facial animation specification, 2026-11-14)
- **Date:** 2026-11-14
- **Affected systems:** model import (`assets`), skinning (`render`), animation (`anim`), the Instance schema (`core`), scripting, replication, the editor

## Context and problem

Paul's spec asks for a hybrid facial rig (blend shapes first, bones for jaw, eyes and tongue), expressions, layered facial animation, procedural blinking and gaze, offline lip sync, import mapping with optional ARKit names, a Character Studio facial workspace, facial LOD and compact replication. The engine had no blend shapes at all: import dropped glTF morph targets and FBX blend deformers, and skinning is done in the vertex shader from a mesh uploaded once and shared by every item.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| **Morph on the GPU in the skinning vertex path, sparse deltas per mesh, weights per item in a shared buffer** | One shared mesh copy stays; cost follows the corners a shape moves; joins skinning's batching | Shader, bind group and bounds work; ray traced passes need their own copy later |
| Morph on the CPU and rewrite a per-character mesh copy (`MeshSource::version`) | Uses an existing path | ~360 KB upload per talking 15k-corner head per frame; no sharing; worse with crowds |
| A new clip type for facial curves | Purpose-built | A second animation format next to `*.anim.json` property tracks, which already animate any property |
| **One `FaceControls` Instance with a number property per standard control** | Property tracks, Sequencer, TweenService, scripts, Properties panel and replication work unchanged | ~55 properties on one class |
| Require ARKit's 52 names on every face | Interoperable | Paul: optional only; stylized faces need fewer shapes |

## Decision

Blend shapes are imported as sparse per-corner deltas and morphed on the GPU before skinning (D1 in `docs/research/facial-animation.md`). Faces are driven through a `FaceControls` Instance whose properties are Axcenity's standard controls (D2); a face profile (`*.face.json`, found by fit) maps controls to a model's shapes and bones, with built-in Axcenity, ARKit, VRM and Oculus-viseme profiles (D3). Facial layers are evaluated after the body pose in a fixed order with configurable layer order (D5). Faces replicate as presentation state, not every control (D8).

## Consequences

- Any glTF or FBX with blend shapes now shows them, with or without a face.
- Ray traced shadows, AO and GI see the unmorphed face until ray tracing poses morphs too.
- The control list is a public contract: renaming a control breaks saved FaceControls and clips, so add, don't rename.

## Implementation references

- `docs/research/facial-animation.md`, stages F1–F7
