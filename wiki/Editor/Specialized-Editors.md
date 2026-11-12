---
title: Specialized editors
type: system
status: current
implementation: implemented
crates: [editor, mesh_edit, anim, blocks, ui]
source: [crates/editor-model-studio/src/lib.rs, crates/editor/src/animation.rs, crates/editor/src/sequencer.rs, crates/editor-ui-designer/src/lib.rs, crates/editor/src/material_editor.rs, crates/editor/src/material_graph.rs, crates/editor/src/block_editor.rs, crates/editor-node-graph/src/lib.rs, crates/editor/src/behavior_tree.rs, crates/editor/src/dialogue_editor.rs, crates/editor/src/data_table.rs, crates/editor/src/terrain_studio.rs]
docs: [docs/editor/model-studio.md, docs/editor/import.md, docs/project/sequences.md, docs/ui.md, docs/editor/assets.md, docs/scripting/blocks.md, docs/scripting/graphs.md, docs/terrain.md, docs/research/usability/usability-audit.md]
verified: 2026-11-11
verified_commit: 19c7179d
tags: [system/editor, tools]
---

# Specialized editors

The editor's tabs beyond the Scene, Explorer and Properties. Each is a module (often a parent file plus a folder) in `crates/editor/src/` with its actions in `app/<name>.rs`; most keep **their own undo** used while the pointer is over the tab. The usability pass of 2026-11-10 (U*, BL*, UD*, AN*, MS* items in `docs/research/usability/`) reworked several of them.

| Tab | Module | Edits | Saves as | Engine side |
|---|---|---|---|---|
| Model Studio | the `editor-model-studio` crate (G11) | Meshes: modeling ops, modifiers, UVs, paint, sculpt, retopology, bake, rig, weights, pose | `.glb` + `.meshedit.json` sidecar | `mesh_edit` ([Mesh editing and CSG](../Engine-Systems/Mesh-Editing-and-CSG.md)) |
| Animation | `animation.rs`, `animation/` | Clips (dope sheet, curves, Record), state machines (Blueprint-style), posing, IK | `*.anim.json`, `*.statemachine.json` | `anim` |
| Sequencer | `sequencer.rs` | Cutscene sequences on a timeline (reuses Animation's dope sheet and curves) | `*.sequence.json` | `anim::sequence`, `script::sequence` |
| UI Designer | the `editor-ui-designer` crate (G11) | ScreenGuis on an artboard (anchors, rulers, palette, inspector), Event Graph, Live Preview | world + `ui/*.uigraph.json` | `ui`, `blocks-runtime::ui_graph` |
| Material | `material_editor.rs`, `material_graph.rs` | Material files: values, maps, presets, shading, water, WGSL shader, material graph | `*.material.json` (graph compiled to WGSL in the editor) | `render` surface shaders |
| Blocks / Graph | `block_editor.rs`, `block_editor/graph.rs`, the `editor-node-graph` crate | Block scripts as blocks or nodes | `*.blocks.json` | `blocks`, `blocks-runtime` |
| Behavior Tree | `behavior_tree.rs` | Behavior trees on the node-graph editor | `*.behaviortree.json` | `nav::behavior` |
| Dialogue | `dialogue_editor.rs` | Dialogues | `*.dialogue.json` | `script::dialogue` |
| Data Table | `data_table.rs` | Data tables | `*.data.json` | `assets::data_table`, `DataService` |
| Terrain Studio | `terrain_studio.rs`, `terrain_studio/` | Sculpt, paint, generate, erode, foliage, layers | terrain folders | `terrain` |

## Model Studio

Phase 55–62: a Blender-like mesh editor inside the engine. Its view draws through `View::use_model` and sculpt chunk meshes (`view::Frame::sculpt`). The Pose test skins exactly as `assets::Posed` does. Phase 90 ("A narrower Model Studio") plans to trim it; not started.

## Animation tab and sequencer

One Animation tab follows the selected Animator; **Animate** (`EditorApp::animate`) makes an Animator, a clip and an Animation as one undo step. Record turns viewport edits into keys. The Sequencer previews in the Scene view through `SequenceRun::apply`, the same code the Sequence Instance runs.

## UI Designer

Lays one ScreenGui out on an artboard (`ui::layout_artboard`), with several selected, drag-to-reparent in the Hierarchy, X/Y/W/H boxes and a beginner-first inspector (usability PRs #44, #48, #52). Its Event Graph tab compiles to the ScreenGui's `EventGraph` LocalScript ([Blocks and graphs](../Engine-Systems/Blocks-and-Graphs.md)).

## Terrain Studio

Tools in Sculpt, Materials and Terrain rows; generation and erosion run as `terrain::job`s on a worker with progress and cancel; foliage is painted from the Assets panel's props and models.

## Shared pieces

- **One node-graph editor** (the `editor-node-graph` crate) serves block Graph view, UI Event Graphs, behavior trees, dialogues and material graphs. Phase 91 ("One graph canvas") plans to finish unifying canvases.
- **Preview canvases** (`animation::preview::Canvas`) run a small World through `scene` so previews draw what Play draws.

## Related documentation

The `docs/editor/*.md` files named above; the usability research in `docs/research/usability/` (`usability-audit.md`, `model-studio.md`, `block-editor.md`, `ui-designer.md`, `design-system.md`, `verification.md`); [Editor architecture](Editor-Architecture.md)
