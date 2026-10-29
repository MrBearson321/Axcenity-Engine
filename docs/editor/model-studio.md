# Model Studio

The Model Studio tab (Phase 55) edits a model's meshes inside the editor and saves them as a `.glb`. The tab is `editor/src/model_studio.rs` (state, toolbar, keys, undo), `model_studio/view.rs` (3D view, picking, handles, overlay), `model_studio/modal.rs` (G, R and S), `model_studio/tools.rs` (modeling operations' keys, menus, the knife, loop cut and adjust panel; Phase 56) and `model_studio/proportional.rs` (proportional editing and pivots); its app actions are `editor/src/app/model_studio.rs`. The meshes are `mesh_edit`'s `EditMesh`, `ModelDoc`, `Selection` and `PrimitiveSettings`; the operations are `mesh_edit::apply` and `MeshOp` (`mesh_edit/src/ops.rs` and `ops/`).

## Opening and New

- **Open** in the tab lists the project's models. **Edit in Model Studio** is on a model's menu in the Assets panel and in a MeshPart's Properties (`Edit::OpenInModelStudio`); a `MeshId` pick (`file.glb#Chair`) makes that object the active one.
- `ModelDoc::open` reads the file through `assets` (`list_objects`, then one `load_models` call with a pick per object), so it reads a file exactly as a MeshPart does. Each object is where it is in the file (node transforms applied), back in the file's units, welded by position into polygons (`mesh_edit::from_model`). Groups stay in the object list.
- A `.glb` saves over itself. An `.fbx` or `.gltf` saves as `<name>.glb` beside it (the original stays).
- **New** makes a cube, plane, UV sphere, ico sphere, cylinder, cone or torus (`PrimitiveSettings`: size, height, thickness, segments, rings, subdivisions; Y up, centred on the origin). It saves as `assets/models/<Shape>.glb` (a number after the name if it's taken).
- Opening while the open model has unsaved changes asks first.

## The mesh

`EditMesh` (`mesh_edit/src/half_edge.rs`) is a half-edge mesh of n-gons: each corner's half-edge carries its uv and normal, so seams and hard edges need no split vertices; each face has a material slot and is smooth or flat. An edge with one face (an open border) or more than two has half-edges without twins. Moves keep vertex and face numbers, so selections hold them; an operation builds a new mesh and leaves its own selection. Moving vertices reshades the faces around them (flat: the face's normal; smooth: the angle-weighted average of the smooth faces at the vertex; with **auto smooth** on the mesh, only the smooth faces reached without crossing an edge sharper than its angle); faces nobody moved keep the file's normals exactly. N-gons are triangulated by ear clipping in their plane (`fill.rs`), and saving writes triangles.

## Modes and picking

- **Object** and **Edit** mode (Tab). Object mode picks objects (click, Shift-click, A) and moves whole objects; Edit mode edits the active object.
- Vertex, Edge, Face (1, 2, 3; switching keeps what's covered). Click, Shift-click (toggles), box (left drag in Edit mode), loop (Alt-click), linked (L), all (A), grow and shrink (Ctrl + / Ctrl − or Ctrl =; taken before egui's zoom).
- Loops (`edge_loop`): straight on across a vertex with four edges; along the border of a face with more than four sides (a cylinder's cap rim); along an open border. In Face mode Alt-click picks the ring of quads the edge crosses (`face_loop`).
- Picking ignores what's hidden behind the mesh (the overlay is X-ray): vertices and edges by distance on screen, faces and objects by ray.

## Moving

- **G, R, S** (`modal.rs`): follow the mouse; X, Y, Z lock to that world axis, again for the Local axis, a third time free; typed numbers (studs, degrees, a factor; `-` flips the sign); Enter or click keeps it, Esc or right-click cancels. G again during a grab is **edge slide**.
- **Pivot** (the toolbar's crosshair menu): Median Point, Bounding Box Center, 3D Cursor, or Individual Origins (each connected part, or in Object mode each object, round its own middle). The handles always go round their box.
- **3D cursor**: Shift+right-click puts it on the surface under the mouse (or level with where it was); Shift+C or the menu's Cursor to World Origin puts it back; Cursor to Selected moves it to the median. Merge At Cursor and Spin use it.
- **Proportional editing** (O, or the dashed circle): in Edit mode, every vertex within the radius of a picked one takes part of the move, by the falloff (Smooth, Sphere, Root, Inverse Square, Sharp, Linear, Constant) of its distance to the nearest picked vertex. During a move the wheel or Page Up/Down changes the radius (the wheel doesn't zoom then), and a circle shows it. Turns turn part of the way (`proportional::partial` slerps); moves and scales go part of the way.
- **Handles**: the Studio toolbar's Move, Rotate and Scale show the editor's own gizmo (`gizmo.rs`) on the box around what's picked. The gizmo drags the view's own MeshPart's pose, and `view::pose_change` turns the change of pose into a matrix for the vertices (a flat side of the box isn't scaled).
- Snapping is the editor toolbar's (Phase 8b): moves in move steps, turns in rotate steps, scales in tenths. Ctrl flips it. **Local** axes put Y along the picked faces' normal (or the picked corners'); Object mode has no local axes.

## Modeling operations (Phase 56)

`mesh_edit::apply(mesh, selection, op)` runs a `MeshOp` and returns the new mesh and what's picked after (the new faces, loops or vertices, in the same select mode), or an error to show. Each takes the mesh apart into a `Soup` of polygons, edits that and builds a new `EditMesh`: faces cleaned of repeated corners (a corner that comes back splits the face), unused vertices dropped, changed faces and their smooth neighbours shaded again.

| Operation | Key | What it does |
|---|---|---|
| Extrude | E | Picked faces pushed out with walls round them: by region (average normal), individual faces, or along normals (even thickness). With no faces, picked open-border edges pull out into faces. |
| Inset | I | A smaller copy inside each region (or face), joined by a ring of quads; depth pushes it out or in. |
| Bevel | Ctrl+B, Ctrl+Shift+B | Edges (between two faces) or corners cut off, width, segments and profile (0.5 round, 1 square, 0.25 flat; a superellipse). Corners move in from both beveled edges, or slide along an unbeveled one; strips run along the edges; a patch fills each corner, except that a lone face with neither edge beveled takes the gap in (a bevel's end), and two strips whose profiles join the same points share them (a beveled loop's mitred corner). Width stops at 0.45 of the shortest edge at a beveled corner. Needs a closed fan of faces round each corner. |
| Loop Cut | Ctrl+R | Hover an edge to see the loop; the wheel sets the cuts; click cuts across the ring of quads it crosses (stopping at other faces or open borders, which take the new points) and then slides. |
| Edge Slide | G G | Picked edges' ends slide along the edges beside them; the sign picks the side. |
| Knife | K | Click on edges or corners (snapping to vertices); Enter or Space cuts each pair in a row across the face they share; Backspace drops a point. Points inside a face aren't supported. |
| Bridge Edge Loops | Mesh menu | Two picked closed loops of open-border edges with as many edges, joined vertex to vertex at the turn that makes the shortest joins. |
| Merge | M | At Center, At Cursor, or By Distance (sorted along X, union-find); faces that end up back to back go. |
| Delete, Dissolve | X or Delete | Delete Vertices, Edges, Faces or Only Faces (keeps the vertices); Dissolve Vertices, Edges or Faces joins the faces round them into one (if their border is one loop), and drops corners left between two edges. |
| Fill, Grid Fill | F, Mesh menu | A face across each picked open-border loop; or a Coons-patch grid of quads across one loop with an even number of edges (span 0 is the squarest; offset turns it). |
| Subdivide | Mesh menu | Picked faces' edges cut in `cuts + 1`: quads into grids, triangles into triangles, other faces into a fan round their middle; neighbours take the points. |
| Spin | Mesh menu | Picked faces (or open-border edges) extruded in steps round an axis through a center (the cursor); a whole turn welds the end to the start and drops the end faces. Points on the axis weld. |
| Flip, Recalculate | Alt+N, Shift+N, Ctrl+Shift+N | Turn faces over; or make each connected part agree with its neighbours and face out (or in), by its signed volume. |
| Shade Smooth, Flat, Auto Smooth | Mesh menu | Picked faces (whole objects in Object mode, the only operations there); Auto Smooth sets the mesh's angle. |

- Every operation keeps a manifold mesh manifold (`mesh_edit::check_manifold`: each edge one or two faces running opposite ways, one fan of faces round each vertex); only Delete opens a closed one. Flipping some faces of a closed mesh makes its edges disagree until Recalculate.
- E, I, Ctrl+B, G G and a loop cut's click run the operation at nothing and then follow the mouse with its main value: the extrude's distance (where the mouse's ray passes nearest the normal's line), the inset's thickness and bevel's width (how much farther from the middle the mouse is), or the slide (sideways). Typing a number sets it; the wheel changes a bevel's segments; click or Enter keeps it; Esc or right-click takes the operation back with nothing to redo. The Mesh menu runs each with settings sized to the mesh.
- **Adjust last operation**: after an operation, a panel at the view's bottom left shows its settings (`tools::settings_ui`). Changing one runs it again from the state before it (`StudioDoc::adjust`), replacing its result without another undo step. Any other edit, a pick, an undo or a mode change closes the panel.

## Undo and saving

- While the mouse is over the tab, Ctrl+Z and Ctrl+Y are the Studio's (`EditorApp::undo` asks `model_studio.has_pointer()` first), and the F3X single-letter tool keys are off. Each move, handle drag, keyboard move or operation is one step; a step puts back the meshes, mode, active object and selection. Picking alone isn't a step.
- **Save** (`save_model_studio`) writes `ModelDoc::write_glb`: a node per object (no transform) under nodes for its groups, so object paths still pick the same objects; a primitive per material slot used; materials deduplicated across objects, with base color, metal-roughness, normal (and scale), occlusion (and strength), emissive textures linked by file relative to the model, alpha mode and cutoff, double-sided, and `KHR_texture_transform` when the uvs are moved. A texture with no file (embedded and never copied out) is left out; importing copies embedded textures out, so imported models keep theirs. The writer is `mesh_edit/src/glb.rs` (serde_json, shared with fracture pieces).
- The save is one step of the editor's history (`History::record_blobs`, "Save <file>"), and `asset_file_changed` reloads the file at once, so MeshParts using it draw the new version that frame. Undoing the save writes the old bytes back and reloads them (`write_package_files`).

## Tests

`mesh_edit/src/studio_tests.rs` (shapes closed and outward, loops on a cylinder, grow and shrink, reshading, a cube and Blender's DamagedHelmet saved and loaded back the same, groups kept), `mesh_edit/src/ops_tests.rs` (each operation on a cube and a cylinder, checked manifold), `editor/src/model_studio/tests.rs` and `modal.rs` (keyboard moves and their undo, a handle drag, picking, snapshots with `AXCENITY_SNAPSHOTS`), `model_studio/tools_tests.rs` (the adjust panel re-running a bevel without another step, values following the mouse, loop cut and knife clicks, proportional editing, pivots, a chair modeled from a cube and its snapshot), `editor/src/app/model_studio_tests.rs` (a New cube saved, a face scaled and saved, the MeshPart drawing it at once; the chair saved as triangles and drawn; an FBX saved as a `.glb`; undo routing).
