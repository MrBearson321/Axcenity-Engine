# Model Studio improvements (R15): audit and stages

Roadmap item R15, section 6.6 of [competitive-development-roadmap.md](competitive-development-roadmap.md). This file checks each of the section's eleven requirements against main (13 Nov 2026, after #271 brought sculpting and baking back) and sets the stages that are left. Each stage is its own PR. What Model Studio does today is [docs/editor/model-studio.md](../editor/model-studio.md).

## Audit

| Requirement | Today | Verdict |
|---|---|---|
| Mesh editing workflows | Phase 56's operations (Extrude, Inset, Bevel, Loop Cut, Edge Slide, Knife, Bridge, Merge, Delete and Dissolve, Fill and Grid Fill, Subdivide, Spin, normals, shading), each with Adjust last operation; G, R, S with axis locks and typed numbers; pivots, the 3D cursor, proportional editing and the gizmo | Met |
| UV editing | Seams and pins, Unwrap (LSCM), Pack, stretch display, islands and overlap warnings, the UV editor with its own G, R, S, stitching and two channels | Met |
| Texture painting brushes | Draw, Soften, Smear, Clone, Fill, Erase; stencil and texture brushes, symmetry, pressure; six maps with layers, blend modes and masks | Met |
| Sculpting tools | Ten brushes (Draw, Clay Strips, Smooth, Grab, Inflate, Crease, Flatten, Pinch, Snake Hook, Mask), symmetry, pressure, masks, Voxel Remesh and Subdivide, about 1 ms a dab at 500k triangles | **Open**: R15.1 |
| Rigging workflows | Rig mode, the Humanoid template, Add Bone, Automatic Weights (bone heat), Pose test, saving to `.glb`; retargeting is G16 | Met |
| Weight painting | Weights mode with its brushes, normalising and mirror | Met |
| Collision generation and editing | None in Model Studio. MeshParts always collide as their convex hull (`physics/src/shapes.rs`, `Fidelity`); only UnionOperations have `CollisionFidelity` | **Built**: R15.2a (#319: `_COL` pieces, MeshPart `CollisionFidelity`) and R15.2b (the Collision fold: Box, Hull, Convex Pieces, Remove, Show Collision) |
| Model LOD management | Generate LODs writes `_LOD1` and `_LOD2` copies (a half and a quarter) that `assets` reads; the view never draws them and their triangle counts aren't shown | **Open**: R15.3 |
| Baking workflows | The Bake fold: a reference sculpt, Make Low Poly, and normal, ambient occlusion, curvature and color maps baked onto the low poly | Met |
| Material previews | The view is the engine's renderer drawing the model as a MeshPart (`view.rs`), so the file's materials show as the scene shows them; the Material tab and Object Viewer preview materials on their own | Met |
| Blender round trip | `.glb` saves and loads back the same (Blender's DamagedHelmet in the tests), and a model saved by another program is read again by the watcher. Nothing opens a model in Blender, and an open Model Studio document isn't told the file changed | **Open**: R15.4 |

The acceptance criteria: common adjustments without leaving Axcenity (met but for collision and LODs, R15.2 and R15.3), previews with the engine's renderer (met), import and export keep supported data (met; R15.2 and R15.3 keep their new objects in the `.glb` by name), and Blender still there for advanced work (R15.4 makes the trip shorter).

## Decisions

- **D1. Sculpting grows by brushes and filters, not by a new mesh.** The `Sculpt` keeps its vertex numbering, so masks, symmetry, undo and the chunked drawing keep working. Dynamic topology (adding triangles under the brush) would break all four and stays out; Voxel Remesh and Subdivide remain the way to add detail.
- **D2. Collision shapes are objects in the model, named like LODs.** An object named `<name>_COL` (or `_COL1`, `_COL2` …, any case) is one convex piece of `<name>`'s collision: the convex hull of its vertices. They're found as `assets` finds `_LOD1` copies, aren't drawn, don't size the model, and are saved in the `.glb` like any object, so Blender can make or edit them too (Unreal's `UCX_` and Godot's `-colonly` are the same idea). MeshParts gain `CollisionFidelity` (Default, Box, Hull, PreciseConvexDecomposition): **Default** uses the file's `_COL` pieces when it has any and its hull otherwise, which is what every MeshPart does today, so nothing changes until a model has pieces or a part asks for another fidelity.
- **D3. LODs stay the renderer's to pick.** R15.3 shows and edits the copies (ratios, counts, a preview of each level); it adds no per-part distances, since the renderer picks by screen size for every model alike (`docs/rendering/pipeline.md`).
- **D4. Blender is launched, not embedded.** A Blender path in Settings (found on PATH or in the usual install folders when empty) and **Edit in Blender** on a model's menus open the `.glb` there through a small import script. Blender's own glTF export back to the same file is picked up by the watcher, and an open Model Studio document of that file offers to load it again (or keeps its unsaved edits). Nothing is installed into Blender.

## Stages

| Stage | What | Tests | PC check |
|---|---|---|---|
| R15.1 Sculpt brushes and filters | Brushes: Layer (raises to a set height once per stroke), Blob (rounded, Draw with Inflate), Scrape and Fill (Flatten only above or only below the plane), Draw Sharp (a crisp line), Nudge (along the stroke on the surface), Elastic Grab (Grab with a soft, volume-keeping falloff). **Mesh Filter**: Smooth, Inflate, Scale, Sharpen and Random over the whole object (masks hold it back), one undo step. Masks saved in the sidecar | Each brush moves what it should on a sphere and leaves the rest; symmetry keeps a mirrored mesh mirrored; filters respect the mask; the mask loads back | Every new brush at 500k triangles against the 1 ms a dab budget |
| R15.2 Collision | D2: `assets` reads `_COL` pieces; physics' MeshPart collider by `CollisionFidelity`; a Collision fold in Model Studio (Generate Box, Generate Hull, Generate Convex Pieces with a piece count, Remove; pieces drawn as wireframe, editable in Edit mode like any object, Show Collision); the Collisions overlay draws them | Pieces found and not drawn; a MeshPart with pieces collides as them (a doorway walked through) and without as before; generated pieces are convex and saved | A model with pieces in Play |
| R15.3 LOD management | The LOD fold: each level's triangle count, ratios per level (default a half and a quarter), Preview LOD 0–2 in the view, Remove LODs, Regenerate | Counts and ratios, preview draws the copy, Remove leaves the full object | — |
| R15.4 Blender round trip | D4: the Blender path setting, Edit in Blender (Assets menu, MeshPart Properties, Model Studio), the import script, and Model Studio's reload offer | The launch command and script for a path; a changed file offers a reload and unsaved edits are kept | Open, change and export in Blender on Windows |

Each stage updates `docs/editor/model-studio.md`, the wiki's Model Studio page and the roadmap's 6.6 status.
