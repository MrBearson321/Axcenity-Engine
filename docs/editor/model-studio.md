# Model Studio

The Model Studio tab (Phase 55) edits a model's meshes inside the editor and saves them as a `.glb`. The tab is `editor/src/model_studio.rs` (state, toolbar, keys, undo), `model_studio/view.rs` (3D view, picking, handles, overlay) and `model_studio/modal.rs` (G, R and S); its app actions are `editor/src/app/model_studio.rs`. The meshes are `mesh_edit`'s `EditMesh`, `ModelDoc`, `Selection` and `PrimitiveSettings`.

## Opening and New

- **Open** in the tab lists the project's models. **Edit in Model Studio** is on a model's menu in the Assets panel and in a MeshPart's Properties (`Edit::OpenInModelStudio`); a `MeshId` pick (`file.glb#Chair`) makes that object the active one.
- `ModelDoc::open` reads the file through `assets` (`list_objects`, then one `load_models` call with a pick per object), so it reads a file exactly as a MeshPart does. Each object is where it is in the file (node transforms applied), back in the file's units, welded by position into polygons (`mesh_edit::from_model`). Groups stay in the object list.
- A `.glb` saves over itself. An `.fbx` or `.gltf` saves as `<name>.glb` beside it (the original stays).
- **New** makes a cube, plane, UV sphere, ico sphere, cylinder, cone or torus (`PrimitiveSettings`: size, height, thickness, segments, rings, subdivisions; Y up, centred on the origin). It saves as `assets/models/<Shape>.glb` (a number after the name if it's taken).
- Opening while the open model has unsaved changes asks first.

## The mesh

`EditMesh` (`mesh_edit/src/half_edge.rs`) is a half-edge mesh of n-gons: each corner's half-edge carries its uv and normal, so seams and hard edges need no split vertices; each face has a material slot and is smooth or flat. An edge with one face (an open border) or more than two has half-edges without twins. Vertices and faces are only added for now, so selections hold their numbers. Moving vertices reshades the faces around them (flat: the face's normal; smooth: the angle-weighted average of the smooth faces at the vertex); faces nobody moved keep the file's normals exactly. N-gons are triangulated by ear clipping in their plane (`fill.rs`).

## Modes and picking

- **Object** and **Edit** mode (Tab). Object mode picks objects (click, Shift-click, A) and moves whole objects; Edit mode edits the active object.
- Vertex, Edge, Face (1, 2, 3; switching keeps what's covered). Click, Shift-click (toggles), box (left drag in Edit mode), loop (Alt-click), linked (L), all (A), grow and shrink (Ctrl + / Ctrl − or Ctrl =; taken before egui's zoom).
- Loops (`edge_loop`): straight on across a vertex with four edges; along the border of a face with more than four sides (a cylinder's cap rim); along an open border. In Face mode Alt-click picks the ring of quads the edge crosses (`face_loop`).
- Picking ignores what's hidden behind the mesh (the overlay is X-ray): vertices and edges by distance on screen, faces and objects by ray.

## Moving

- **G, R, S** (`modal.rs`): follow the mouse; X, Y, Z lock to that world axis, again for the Local axis, a third time free; typed numbers (studs, degrees, a factor; `-` flips the sign); Enter or click keeps it, Esc or right-click cancels. Moves go around the median of what's picked.
- **Handles**: the Studio toolbar's Move, Rotate and Scale show the editor's own gizmo (`gizmo.rs`) on the box around what's picked. The gizmo drags the view's own MeshPart's pose, and `view::pose_change` turns the change of pose into a matrix for the vertices (a flat side of the box isn't scaled).
- Snapping is the editor toolbar's (Phase 8b): moves in move steps, turns in rotate steps, scales in tenths. Ctrl flips it. **Local** axes put Y along the picked faces' normal (or the picked corners'); Object mode has no local axes.

## Undo and saving

- While the mouse is over the tab, Ctrl+Z and Ctrl+Y are the Studio's (`EditorApp::undo` asks `model_studio.has_pointer()` first), and the F3X single-letter tool keys are off. Each move, handle drag or keyboard move is one step; a step puts back the meshes, mode, active object and selection. Picking alone isn't a step.
- **Save** (`save_model_studio`) writes `ModelDoc::write_glb`: a node per object (no transform) under nodes for its groups, so object paths still pick the same objects; a primitive per material slot used; materials deduplicated across objects, with base color, metal-roughness, normal (and scale), occlusion (and strength), emissive textures linked by file relative to the model, alpha mode and cutoff, double-sided, and `KHR_texture_transform` when the uvs are moved. A texture with no file (embedded and never copied out) is left out; importing copies embedded textures out, so imported models keep theirs. The writer is `mesh_edit/src/glb.rs` (serde_json, shared with fracture pieces).
- The save is one step of the editor's history (`History::record_blobs`, "Save <file>"), and `asset_file_changed` reloads the file at once, so MeshParts using it draw the new version that frame. Undoing the save writes the old bytes back and reloads them (`write_package_files`).

## Tests

`mesh_edit/src/studio_tests.rs` (shapes closed and outward, loops on a cylinder, grow and shrink, reshading, a cube and Blender's DamagedHelmet saved and loaded back the same, groups kept), `editor/src/model_studio/tests.rs` and `modal.rs` (keyboard moves and their undo, a handle drag, picking, snapshots with `AXCENITY_SNAPSHOTS`), `editor/src/app/model_studio_tests.rs` (a New cube saved, a face scaled and saved, the MeshPart drawing it at once; an FBX saved as a `.glb`; undo routing).
