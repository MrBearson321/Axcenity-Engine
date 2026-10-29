//! Phase 56: the modeling operations, each run on a cube and a cylinder,
//! keeping them manifold.

use std::collections::BTreeSet;

use glam::Vec3;

use crate::{
    DeleteKind, DissolveKind, EditMesh, ExtrudeMode, KnifePoint, MergeAt, MeshOp, Polygon,
    PrimitiveSettings, PrimitiveShape, SelectMode, Selection, apply, check_manifold,
};

fn cube() -> EditMesh {
    PrimitiveSettings::of(PrimitiveShape::Cube).build()
}

/// A 12-sided cylinder, 2 across and 2 tall, in two bands.
fn cylinder() -> EditMesh {
    let mut settings = PrimitiveSettings::of(PrimitiveShape::Cylinder);
    settings.segments = 12;
    settings.rings = 2;
    settings.build()
}

/// The space a closed mesh encloses (positive when its faces face out).
fn volume(mesh: &EditMesh) -> f32 {
    (0..mesh.face_count() as u32)
        .flat_map(|face| mesh.triangles(face))
        .map(|t| {
            let [a, b, c] = t.map(|e| mesh.position(mesh.half_edge(e).vertex));
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

/// The faces whose normals point within a few degrees of `direction`.
fn faces_facing(mesh: &EditMesh, direction: Vec3) -> Selection {
    let mut selection = Selection {
        mode: SelectMode::Face,
        ..Selection::default()
    };
    selection.faces = (0..mesh.face_count() as u32)
        .filter(|&f| mesh.face_normal(f).dot(direction) > 0.99)
        .collect();
    assert!(!selection.faces.is_empty(), "no face faces {direction}");
    selection
}

fn all_faces(mesh: &EditMesh) -> Selection {
    Selection {
        mode: SelectMode::Face,
        faces: (0..mesh.face_count() as u32).collect(),
        ..Selection::default()
    }
}

/// The edges (keys) with both ends where `keep` says.
fn edges_where(mesh: &EditMesh, keep: impl Fn(Vec3, Vec3) -> bool) -> Selection {
    let mut selection = Selection {
        mode: SelectMode::Edge,
        ..Selection::default()
    };
    selection.edges = mesh
        .edges()
        .into_iter()
        .filter(|&e| {
            let (a, b) = mesh.ends(e);
            keep(mesh.position(a), mesh.position(b))
        })
        .collect();
    assert!(!selection.edges.is_empty(), "no edges picked");
    selection
}

fn vertices_where(mesh: &EditMesh, keep: impl Fn(Vec3) -> bool) -> Selection {
    Selection {
        mode: SelectMode::Vertex,
        vertices: (0..mesh.vertex_count() as u32)
            .filter(|&v| keep(mesh.position(v)))
            .collect(),
        ..Selection::default()
    }
}

fn run(mesh: &EditMesh, selection: &Selection, op: &MeshOp) -> (EditMesh, Selection) {
    apply(mesh, selection, op).unwrap_or_else(|error| panic!("{op:?}: {error}"))
}

/// Closed, manifold, facing out, with no vertex left unused.
fn assert_closed(mesh: &EditMesh, what: &str) {
    check_manifold(mesh, true).unwrap_or_else(|error| panic!("{what}: {error}"));
    assert!(volume(mesh) > 0.0, "{what} faces inward");
    let used: BTreeSet<u32> = mesh.half_edges().iter().map(|e| e.vertex).collect();
    assert_eq!(used.len(), mesh.vertex_count(), "{what} has loose vertices");
}

/// V − E + F, 2 for anything shaped like a ball.
fn euler(mesh: &EditMesh) -> i64 {
    mesh.vertex_count() as i64 - mesh.edges().len() as i64 + mesh.face_count() as i64
}

fn top_rim(p: Vec3, q: Vec3) -> bool {
    p.y > 0.99 && q.y > 0.99
}

#[test]
fn extrude_keeps_a_cube_and_a_cylinder_closed_in_every_mode() {
    let cube = cube();
    let (out, picked) = run(
        &cube,
        &faces_facing(&cube, Vec3::Y),
        &MeshOp::Extrude {
            mode: ExtrudeMode::Region,
            distance: 1.0,
        },
    );
    assert_closed(&out, "cube extruded");
    assert_eq!((out.vertex_count(), out.face_count()), (12, 10));
    assert!((volume(&out) - 12.0).abs() < 1e-4);
    // The pushed-out face is still picked, a stud higher.
    let [top] = picked.faces.iter().copied().collect::<Vec<_>>()[..] else {
        panic!("one face picked");
    };
    assert!((out.face_center(top).y - 2.0).abs() < 1e-5);

    for mode in [ExtrudeMode::Individual, ExtrudeMode::AlongNormals] {
        let mut two = faces_facing(&cube, Vec3::Y);
        two.faces.extend(faces_facing(&cube, Vec3::X).faces);
        let (out, _) = run(
            &cube,
            &two,
            &MeshOp::Extrude {
                mode,
                distance: 0.5,
            },
        );
        assert_closed(&out, &format!("cube extruded {mode:?}"));
        assert_eq!(euler(&out), 2);
    }

    let cylinder = cylinder();
    let (out, _) = run(
        &cylinder,
        &faces_facing(&cylinder, Vec3::Y),
        &MeshOp::Extrude {
            mode: ExtrudeMode::Region,
            distance: 1.0,
        },
    );
    assert_closed(&out, "cylinder extruded");
    assert!(volume(&out) > volume(&cylinder) + 2.5);

    // Edges on an open border pull out into new faces.
    let (open, _) = run(
        &cylinder,
        &faces_facing(&cylinder, Vec3::Y),
        &MeshOp::Delete(DeleteKind::Faces),
    );
    let (out, _) = run(
        &open,
        &edges_where(&open, top_rim),
        &MeshOp::Extrude {
            mode: ExtrudeMode::Region,
            distance: 0.5,
        },
    );
    check_manifold(&out, false).unwrap();
    assert_eq!(out.face_count(), open.face_count() + 12);
}

#[test]
fn inset_keeps_a_cube_and_a_cylinder_closed() {
    let cube = cube();
    let (out, picked) = run(
        &cube,
        &faces_facing(&cube, Vec3::Y),
        &MeshOp::Inset {
            thickness: 0.25,
            depth: 0.0,
            individual: false,
        },
    );
    assert_closed(&out, "cube inset");
    assert_eq!((out.vertex_count(), out.face_count()), (12, 10));
    let top = *picked.faces.first().unwrap();
    let corners = out.face_vertices(top);
    assert!(
        corners
            .iter()
            .all(|&v| (out.position(v).x.abs() - 0.75).abs() < 1e-5)
    );

    let mut two = faces_facing(&cube, Vec3::Y);
    two.faces.extend(faces_facing(&cube, Vec3::Z).faces);
    for individual in [false, true] {
        let (out, _) = run(
            &cube,
            &two,
            &MeshOp::Inset {
                thickness: 0.2,
                depth: -0.1,
                individual,
            },
        );
        assert_closed(
            &out,
            &format!("two cube faces inset, individual {individual}"),
        );
    }

    let cylinder = cylinder();
    let (out, _) = run(
        &cylinder,
        &faces_facing(&cylinder, Vec3::Y),
        &MeshOp::Inset {
            thickness: 0.3,
            depth: 0.2,
            individual: false,
        },
    );
    assert_closed(&out, "cylinder cap inset");
    assert_eq!(out.face_count(), cylinder.face_count() + 12);
}

#[test]
fn bevel_rounds_edges_and_corners_and_keeps_the_mesh_closed() {
    let cube = cube();
    let one_edge = edges_where(&cube, |p, q| {
        p.y > 0.9 && q.y > 0.9 && p.z > 0.9 && q.z > 0.9
    });
    for segments in [1, 3] {
        let (out, _) = run(
            &cube,
            &one_edge,
            &MeshOp::Bevel {
                width: 0.25,
                segments,
                profile: 0.5,
                vertices: false,
            },
        );
        assert_closed(&out, &format!("one cube edge beveled, {segments} segments"));
        assert_eq!(out.face_count(), 6 + segments as usize);
        assert!(volume(&out) < 8.0);
    }

    // Every edge, and the top loop.
    let every = edges_where(&cube, |_, _| true);
    let (out, _) = run(
        &cube,
        &every,
        &MeshOp::Bevel {
            width: 0.2,
            segments: 2,
            profile: 0.5,
            vertices: false,
        },
    );
    assert_closed(&out, "every cube edge beveled");
    assert_eq!(euler(&out), 2);
    let (out, _) = run(
        &cube,
        &edges_where(&cube, top_rim),
        &MeshOp::Bevel {
            width: 0.3,
            segments: 1,
            profile: 0.5,
            vertices: false,
        },
    );
    assert_closed(&out, "the cube's top loop beveled");
    // Four strips round the top; no corner patches.
    assert_eq!(out.face_count(), 10);
    // Rounded, the strips meet mitred at the corners, still with no
    // patches.
    let (out, _) = run(
        &cube,
        &edges_where(&cube, top_rim),
        &MeshOp::Bevel {
            width: 0.3,
            segments: 3,
            profile: 0.5,
            vertices: false,
        },
    );
    assert_closed(&out, "the cube's top loop rounded");
    assert_eq!(out.face_count(), 6 + 4 * 3);

    // A corner cut off.
    let corner = vertices_where(&cube, |p| p.cmpgt(Vec3::ZERO).all());
    for segments in [1, 2] {
        let (out, _) = run(
            &cube,
            &corner,
            &MeshOp::Bevel {
                width: 0.5,
                segments,
                profile: 0.5,
                vertices: true,
            },
        );
        assert_closed(&out, "a cube corner beveled");
        assert_eq!(out.face_count(), 7);
    }

    let cylinder = cylinder();
    let (out, _) = run(
        &cylinder,
        &edges_where(&cylinder, top_rim),
        &MeshOp::Bevel {
            width: 0.2,
            segments: 3,
            profile: 0.5,
            vertices: false,
        },
    );
    assert_closed(&out, "the cylinder's rim beveled");
    assert_eq!(out.face_count(), cylinder.face_count() + 3 * 12);
}

#[test]
fn loop_cut_and_edge_slide_keep_a_cube_and_a_cylinder_closed() {
    let cube = cube();
    let edge = cube.edges()[0];
    let (a, b) = cube.ends(edge);
    let (out, picked) = run(
        &cube,
        &Selection::default(),
        &MeshOp::LoopCut {
            edge: [a, b],
            cuts: 2,
            slide: 0.0,
        },
    );
    assert_closed(&out, "cube loop cut");
    assert_eq!(out.face_count(), 6 + 4 * 2);
    assert_eq!(picked.vertices.len(), 8);

    let cylinder = cylinder();
    let side = cylinder
        .edges()
        .into_iter()
        .find(|&e| {
            let (a, b) = cylinder.ends(e);
            (cylinder.position(a).y - cylinder.position(b).y).abs() > 0.5
        })
        .unwrap();
    let (a, b) = cylinder.ends(side);
    let selection = Selection {
        mode: SelectMode::Edge,
        ..Selection::default()
    };
    let (out, picked) = run(
        &cylinder,
        &selection,
        &MeshOp::LoopCut {
            edge: [a, b],
            cuts: 1,
            slide: 0.5,
        },
    );
    assert_closed(&out, "cylinder loop cut");
    // A ring round all 12 sides of one band, slid a quarter of the way.
    assert_eq!(out.face_count(), cylinder.face_count() + 12);
    assert_eq!(picked.edges.len(), 12);
    let heights: BTreeSet<i32> = picked
        .vertices_of(&out)
        .iter()
        .map(|&v| (out.position(v).y * 1000.0).round() as i32)
        .collect();
    assert_eq!(heights.len(), 1, "the cut loop is level");

    // The new loop slid halfway toward one side, then the other.
    let y =
        |mesh: &EditMesh, selection: &Selection| mesh.position(selection.vertices_of(mesh)[0]).y;
    let before = y(&out, &picked);
    for factor in [0.5, -0.5] {
        let (slid, _) = run(&out, &picked, &MeshOp::EdgeSlide { factor });
        assert_closed(&slid, "cylinder edge slid");
        assert!((y(&slid, &picked) - before).abs() > 0.1);
    }

    // A cube edge slid along its faces.
    let top = edges_where(&cube, |p, q| top_rim(p, q) && p.z > 0.9 && q.z > 0.9);
    let (slid, _) = run(&cube, &top, &MeshOp::EdgeSlide { factor: 0.5 });
    assert_closed(&slid, "cube edge slid");
    assert!(volume(&slid) < 8.0);
}

#[test]
fn the_knife_cuts_faces_along_its_line() {
    let cube = cube();
    let top = faces_facing(&cube, Vec3::Y);
    let face = *top.faces.first().unwrap();
    let corners = cube.face_vertices(face);
    // From the middle of one side of the top to the middle of the
    // opposite one.
    let points = vec![
        KnifePoint {
            a: corners[0],
            b: corners[1],
            t: 0.5,
        },
        KnifePoint {
            a: corners[2],
            b: corners[3],
            t: 0.5,
        },
    ];
    let (out, _) = run(&cube, &top, &MeshOp::Knife { points });
    assert_closed(&out, "cube cut by the knife");
    assert_eq!((out.vertex_count(), out.face_count()), (10, 7));

    let cylinder = cylinder();
    let cap = faces_facing(&cylinder, Vec3::Y);
    let face = *cap.faces.first().unwrap();
    let rim = cylinder.face_vertices(face);
    let points = vec![
        KnifePoint {
            a: rim[0],
            b: rim[0],
            t: 0.0,
        },
        KnifePoint {
            a: rim[6],
            b: rim[7],
            t: 0.25,
        },
    ];
    let (out, _) = run(&cylinder, &cap, &MeshOp::Knife { points });
    assert_closed(&out, "cylinder cap cut by the knife");
    assert_eq!(out.face_count(), cylinder.face_count() + 1);
}

/// `mesh` and a copy of it `offset` away, as one mesh.
fn two_apart(mesh: &EditMesh, offset: Vec3) -> EditMesh {
    let count = mesh.vertex_count() as u32;
    let positions: Vec<Vec3> = mesh
        .positions()
        .iter()
        .copied()
        .chain(mesh.positions().iter().map(|&p| p + offset))
        .collect();
    let polygons: Vec<Polygon> = (0..2)
        .flat_map(|copy| {
            (0..mesh.face_count() as u32).map(move |f| Polygon {
                corners: mesh
                    .face_vertices(f)
                    .iter()
                    .map(|v| v + copy * count)
                    .collect(),
                ..Polygon::default()
            })
        })
        .collect();
    EditMesh::new(positions, &polygons)
}

#[test]
fn bridge_joins_two_open_loops() {
    for (name, mesh) in [("cube", cube()), ("cylinder", cylinder())] {
        let pair = two_apart(&mesh, Vec3::new(0.0, 4.0, 0.0));
        // The faces facing each other go.
        let facing = faces_facing(&pair, Vec3::Y)
            .faces
            .into_iter()
            .filter(|&f| pair.face_center(f).y < 2.0)
            .chain(
                faces_facing(&pair, Vec3::NEG_Y)
                    .faces
                    .into_iter()
                    .filter(|&f| pair.face_center(f).y > 2.0),
            )
            .collect();
        let gap = Selection {
            mode: SelectMode::Face,
            faces: facing,
            ..Selection::default()
        };
        let (open, _) = run(&pair, &gap, &MeshOp::Delete(DeleteKind::Faces));
        check_manifold(&open, false).unwrap();
        let rims = edges_where(&open, |p, q| {
            (p.y - 1.0).abs() < 1e-4 && (q.y - 1.0).abs() < 1e-4
                || (p.y - 3.0).abs() < 1e-4 && (q.y - 3.0).abs() < 1e-4
        });
        let (out, _) = run(&open, &rims, &MeshOp::Bridge);
        assert_closed(&out, &format!("two {name}s bridged"));
        assert_eq!(euler(&out), 2, "{name}");
        // The gap between them is as tall as each one.
        assert!((volume(&out) - 3.0 * volume(&mesh)).abs() < 1e-3);
    }
}

#[test]
fn merge_joins_vertices_at_the_middle_the_cursor_or_by_distance() {
    let cube = cube();
    let top = vertices_where(&cube, |p| p.y > 0.9);
    let (out, picked) = run(
        &cube,
        &top,
        &MeshOp::Merge {
            at: MergeAt::Center,
            cursor: Vec3::ZERO,
            distance: 0.0,
        },
    );
    assert_closed(&out, "cube top merged");
    assert_eq!((out.vertex_count(), out.face_count()), (5, 5));
    let apex = *picked.vertices.first().unwrap();
    assert!(out.position(apex).abs_diff_eq(Vec3::Y, 1e-6));

    let cylinder = cylinder();
    let (out, _) = run(
        &cylinder,
        &vertices_where(&cylinder, |p| p.y > 0.9),
        &MeshOp::Merge {
            at: MergeAt::Cursor,
            cursor: Vec3::new(0.0, 3.0, 0.0),
            distance: 0.0,
        },
    );
    assert_closed(&out, "cylinder top merged at the cursor");
    assert_eq!(out.vertex_count(), cylinder.vertex_count() - 11);

    // A zero-length extrude welded away again.
    for mesh in [cube, cylinder] {
        let (doubled, _) = run(
            &mesh,
            &faces_facing(&mesh, Vec3::Y),
            &MeshOp::Extrude {
                mode: ExtrudeMode::Region,
                distance: 0.0,
            },
        );
        let everything = vertices_where(&doubled, |_| true);
        let (out, _) = run(
            &doubled,
            &everything,
            &MeshOp::Merge {
                at: MergeAt::Distance,
                cursor: Vec3::ZERO,
                distance: 1e-4,
            },
        );
        assert_closed(&out, "doubles merged");
        assert_eq!(
            (out.vertex_count(), out.face_count()),
            (mesh.vertex_count(), mesh.face_count())
        );
    }
}

#[test]
fn dissolve_joins_faces_and_keeps_the_mesh_closed() {
    let cube = cube();
    let (cut, _) = run(
        &cube,
        &faces_facing(&cube, Vec3::Y),
        &MeshOp::Subdivide { cuts: 1 },
    );
    assert_closed(&cut, "cube top subdivided");
    let (out, picked) = run(
        &cut,
        &faces_facing(&cut, Vec3::Y),
        &MeshOp::Dissolve(DissolveKind::Faces),
    );
    assert_closed(&out, "cube top dissolved");
    assert_eq!(out.face_count(), 6);
    assert_eq!(picked.faces.len(), 1);

    // The middle vertex of the subdivided top.
    let middle = vertices_where(&cut, |p| p.abs_diff_eq(Vec3::Y, 1e-5));
    let (out, _) = run(&cut, &middle, &MeshOp::Dissolve(DissolveKind::Vertices));
    assert_closed(&out, "cube top's middle dissolved");
    assert_eq!(out.face_count(), 6);
    assert_eq!(out.vertex_count(), cut.vertex_count() - 1);

    // A loop cut, then its loop dissolved: the cube again.
    let (a, b) = cube.ends(cube.edges()[0]);
    let (looped, picked) = run(
        &cube,
        &Selection {
            mode: SelectMode::Edge,
            ..Selection::default()
        },
        &MeshOp::LoopCut {
            edge: [a, b],
            cuts: 1,
            slide: 0.0,
        },
    );
    let (out, _) = run(&looped, &picked, &MeshOp::Dissolve(DissolveKind::Edges));
    assert_closed(&out, "loop dissolved");
    assert_eq!((out.vertex_count(), out.face_count()), (8, 6));

    let cylinder = cylinder();
    let side = edges_where(&cylinder, |p, q| {
        (p.y - q.y).abs() > 0.5 && p.y.min(q.y) > -0.01 && p.x > 0.99 && q.x > 0.99
    });
    let (out, _) = run(&cylinder, &side, &MeshOp::Dissolve(DissolveKind::Edges));
    assert_closed(&out, "cylinder side edge dissolved");
    assert_eq!(out.face_count(), cylinder.face_count() - 1);
}

#[test]
fn delete_leaves_an_open_but_manifold_mesh() {
    for (name, mesh) in [("cube", cube()), ("cylinder", cylinder())] {
        let top = faces_facing(&mesh, Vec3::Y);
        for kind in [DeleteKind::Faces, DeleteKind::OnlyFaces] {
            let (out, _) = run(&mesh, &top, &MeshOp::Delete(kind));
            check_manifold(&out, false).unwrap_or_else(|e| panic!("{name} {kind:?}: {e}"));
            assert!(check_manifold(&out, true).is_err());
            assert_eq!(out.face_count(), mesh.face_count() - 1);
        }
        let (out, _) = run(
            &mesh,
            &vertices_where(&mesh, |p| p.y > 0.9 && p.x > 0.9),
            &MeshOp::Delete(DeleteKind::Vertices),
        );
        check_manifold(&out, false).unwrap_or_else(|e| panic!("{name} vertex: {e}"));
        assert!(out.vertex_count() < mesh.vertex_count());
        let (out, _) = run(
            &mesh,
            &edges_where(&mesh, top_rim),
            &MeshOp::Delete(DeleteKind::Edges),
        );
        check_manifold(&out, false).unwrap_or_else(|e| panic!("{name} edges: {e}"));
    }
}

#[test]
fn fill_and_grid_fill_close_holes() {
    for (name, mesh) in [("cube", cube()), ("cylinder", cylinder())] {
        let (open, _) = run(
            &mesh,
            &faces_facing(&mesh, Vec3::Y),
            &MeshOp::Delete(DeleteKind::Faces),
        );
        let rim = edges_where(&open, top_rim);
        let (out, _) = run(&open, &rim, &MeshOp::Fill);
        assert_closed(&out, &format!("{name} filled"));
        assert!((volume(&out) - volume(&mesh)).abs() < 1e-4);

        let (out, picked) = run(&open, &rim, &MeshOp::GridFill { span: 0, offset: 0 });
        assert_closed(&out, &format!("{name} grid filled"));
        assert!((volume(&out) - volume(&mesh)).abs() < 1e-3);
        // All quads.
        assert!(
            picked
                .vertices_of(&out)
                .iter()
                .all(|&v| out.position(v).y > 0.99)
        );
        let quads = (0..out.face_count() as u32)
            .filter(|&f| out.face_normal(f).y > 0.99)
            .all(|f| out.face_vertices(f).len() == 4);
        assert!(quads, "{name}");
    }
    // 12 edges round: 3 by 3 quads.
    let cylinder = cylinder();
    let (open, _) = run(
        &cylinder,
        &faces_facing(&cylinder, Vec3::Y),
        &MeshOp::Delete(DeleteKind::Faces),
    );
    let (out, _) = run(
        &open,
        &edges_where(&open, top_rim),
        &MeshOp::GridFill { span: 3, offset: 1 },
    );
    assert_eq!(out.face_count(), open.face_count() + 9);
}

#[test]
fn subdivide_cuts_quads_triangles_and_ngons() {
    let cube = cube();
    for (cuts, faces) in [(1, 24), (2, 54)] {
        let (out, _) = run(&cube, &all_faces(&cube), &MeshOp::Subdivide { cuts });
        assert_closed(&out, "cube subdivided");
        assert_eq!(out.face_count(), faces);
        assert!((volume(&out) - 8.0).abs() < 1e-4);
    }
    let cylinder = cylinder();
    let (out, _) = run(
        &cylinder,
        &all_faces(&cylinder),
        &MeshOp::Subdivide { cuts: 2 },
    );
    assert_closed(&out, "cylinder subdivided");
    // Only the caps' edges cut: the bands next door take the points.
    let (out, _) = run(
        &cylinder,
        &faces_facing(&cylinder, Vec3::Y),
        &MeshOp::Subdivide { cuts: 1 },
    );
    assert_closed(&out, "cylinder cap subdivided");
    let cone = PrimitiveSettings::of(PrimitiveShape::Cone).build();
    let (out, _) = run(&cone, &all_faces(&cone), &MeshOp::Subdivide { cuts: 3 });
    assert_closed(&out, "cone (triangles) subdivided");
}

#[test]
fn spin_sweeps_faces_and_edges_round_an_axis() {
    let cube = cube();
    let (out, _) = run(
        &cube,
        &faces_facing(&cube, Vec3::Y),
        &MeshOp::Spin {
            degrees: 90.0,
            steps: 4,
            axis: Vec3::NEG_Z,
            center: Vec3::new(2.0, 1.0, 0.0),
        },
    );
    assert_closed(&out, "cube top spun");
    assert_eq!(out.face_count(), 6 + 4 * 4);

    let cylinder = cylinder();
    let (out, _) = run(
        &cylinder,
        &faces_facing(&cylinder, Vec3::Y),
        &MeshOp::Spin {
            degrees: 120.0,
            steps: 6,
            axis: Vec3::X,
            center: Vec3::new(0.0, 1.0, 2.0),
        },
    );
    assert_closed(&out, "cylinder top spun");

    // A square with one side on the axis, a whole turn round it: a solid
    // cylinder.
    let plane = PrimitiveSettings::of(PrimitiveShape::Plane).build();
    let (out, _) = run(
        &plane,
        &all_faces(&plane),
        &MeshOp::Spin {
            degrees: 360.0,
            steps: 8,
            axis: Vec3::Z,
            center: Vec3::new(-1.0, 0.0, 0.0),
        },
    );
    assert_closed(&out, "a square spun round its side");
    assert_eq!(euler(&out), 2);
    // A lathe: the open border's far edge swept half a turn.
    let far = edges_where(&plane, |p, q| p.x > 0.99 && q.x > 0.99);
    let (out, _) = run(
        &plane,
        &far,
        &MeshOp::Spin {
            degrees: 180.0,
            steps: 6,
            axis: Vec3::Z,
            center: Vec3::new(-1.0, 0.0, 0.0),
        },
    );
    check_manifold(&out, false).unwrap();
    assert_eq!(out.face_count(), 1 + 6);
}

#[test]
fn flip_and_recalculate_turn_faces_the_right_way() {
    for (name, mesh) in [("cube", cube()), ("cylinder", cylinder())] {
        let (out, _) = run(&mesh, &all_faces(&mesh), &MeshOp::FlipNormals);
        check_manifold(&out, true).unwrap();
        assert!(volume(&out) < 0.0, "{name} flipped");
        let (back, _) = run(
            &out,
            &all_faces(&out),
            &MeshOp::RecalculateNormals { inside: false },
        );
        assert_closed(&back, &format!("{name} recalculated"));

        // One face flipped is put right again.
        let (one, _) = run(&mesh, &faces_facing(&mesh, Vec3::Y), &MeshOp::FlipNormals);
        assert!(check_manifold(&one, true).is_err());
        let (fixed, _) = run(
            &one,
            &all_faces(&one),
            &MeshOp::RecalculateNormals { inside: false },
        );
        assert_closed(&fixed, &format!("{name} with one face put right"));
        let (inside, _) = run(
            &one,
            &all_faces(&one),
            &MeshOp::RecalculateNormals { inside: true },
        );
        check_manifold(&inside, true).unwrap();
        assert!(volume(&inside) < 0.0);
    }
}

#[test]
fn shading_smooth_flat_and_auto_smooth() {
    let cylinder = cylinder();
    let (flat, _) = run(&cylinder, &all_faces(&cylinder), &MeshOp::ShadeFlat);
    assert_closed(&flat, "shaded flat");
    assert!((0..flat.face_count() as u32).all(|f| !flat.face(f).smooth));

    // Auto smooth at 40°: the sides (30° apart) shade round, but the rim
    // (90°) stays sharp.
    let (auto, _) = run(
        &cylinder,
        &all_faces(&cylinder),
        &MeshOp::AutoSmooth { degrees: 40.0 },
    );
    assert_closed(&auto, "auto smooth");
    assert!(auto.auto_smooth().is_some());
    for edge in auto.half_edges() {
        let p = auto.position(edge.vertex);
        if p.y < 0.99 {
            continue;
        }
        let face_normal = auto.face_normal(edge.face);
        if face_normal.y > 0.99 {
            assert!(edge.normal.abs_diff_eq(Vec3::Y, 1e-5), "the cap stays flat");
        } else {
            // Along the side, out from the axis.
            let out = Vec3::new(p.x, 0.0, p.z).normalize();
            assert!(
                edge.normal.abs_diff_eq(out, 1e-4),
                "{} vs {out}",
                edge.normal
            );
        }
    }
    let (smooth, _) = run(&auto, &all_faces(&auto), &MeshOp::ShadeSmooth);
    assert!(smooth.auto_smooth().is_none());
    let cap_corner = smooth
        .half_edges()
        .iter()
        .find(|e| smooth.face_normal(e.face).y > 0.99)
        .unwrap();
    assert!(
        cap_corner.normal.y < 0.99,
        "smooth everywhere blends the rim"
    );
}
