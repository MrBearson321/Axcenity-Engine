//! Phase 58: seams, unwrapping, projections, packing and the UV editor's
//! edits, and uvs kept through operations, the `.glb` and the sidecar.

use std::collections::BTreeSet;

use axcenity_assets::ModelFormat;
use glam::{Affine2, Mat4, Vec2, Vec3};

use crate::uv::{self, AlignAxis, Projection, triangles_overlap};
use crate::{
    EditMesh, ExtrudeMode, MeshOp, ModelDoc, PrimitiveSettings, PrimitiveShape, SelectMode,
    Selection, apply,
};

fn cube() -> EditMesh {
    PrimitiveSettings::default().build()
}

fn all_faces(mesh: &EditMesh) -> Vec<u32> {
    (0..mesh.face_count() as u32).collect()
}

/// Every edge of `mesh` by its two vertices.
fn edges(mesh: &EditMesh) -> Vec<[u32; 2]> {
    mesh.edges()
        .into_iter()
        .map(|e| {
            let (a, b) = mesh.ends(e);
            [a.min(b), a.max(b)]
        })
        .collect()
}

/// The face of `mesh` facing `normal`.
fn face_facing(mesh: &EditMesh, normal: Vec3) -> u32 {
    (0..mesh.face_count() as u32)
        .find(|&f| mesh.face_normal(f).dot(normal) > 0.99)
        .unwrap()
}

/// The edge two faces share.
fn shared_edge(mesh: &EditMesh, a: u32, b: u32) -> [u32; 2] {
    let of = |face: u32| -> BTreeSet<u32> { mesh.face_vertices(face).into_iter().collect() };
    let both: Vec<u32> = of(a).intersection(&of(b)).copied().collect();
    assert_eq!(both.len(), 2);
    [both[0], both[1]]
}

/// `face`'s corner uvs, in order round it.
fn face_uvs(mesh: &EditMesh, face: u32) -> Vec<Vec2> {
    mesh.face_half_edges(face)
        .into_iter()
        .map(|e| mesh.uv(e, 0))
        .collect()
}

/// The side of `face` if its uvs make a square, else a panic.
fn square_side(mesh: &EditMesh, face: u32) -> f32 {
    let uvs = face_uvs(mesh, face);
    assert_eq!(uvs.len(), 4);
    let side = uvs[0].distance(uvs[1]);
    for k in 0..4 {
        let (a, b, c) = (uvs[k], uvs[(k + 1) % 4], uvs[(k + 2) % 4]);
        assert!(
            (a.distance(b) - side).abs() < side * 1e-3,
            "face {face}: {uvs:?}"
        );
        assert!(
            (b - a).dot(c - b).abs() < side * side * 1e-3,
            "face {face}: a right angle"
        );
    }
    side
}

/// Panics unless `face`'s uvs meet at right angles, lined up with the
/// texture.
fn square_corners(mesh: &EditMesh, face: u32) {
    let uvs = face_uvs(mesh, face);
    for k in 0..uvs.len() {
        let edge = uvs[(k + 1) % uvs.len()] - uvs[k];
        assert!(
            edge.x.abs() < 1e-4 || edge.y.abs() < 1e-4,
            "face {face}: {uvs:?}"
        );
    }
}

/// True if no two triangles of different faces overlap in uv space.
fn faces_overlap(mesh: &EditMesh) -> bool {
    let triangles: Vec<(u32, [Vec2; 3])> = (0..mesh.face_count() as u32)
        .flat_map(|f| mesh.triangles(f).into_iter().map(move |t| (f, t)))
        .map(|(f, t)| (f, t.map(|e| mesh.uv(e, 0))))
        .collect();
    triangles.iter().any(|(f, s)| {
        triangles
            .iter()
            .any(|(g, t)| f < g && triangles_overlap(s, t))
    })
}

fn in_texture(mesh: &EditMesh) -> bool {
    mesh.half_edges()
        .iter()
        .all(|e| e.uv.cmpge(Vec2::splat(-1e-5)).all() && e.uv.cmple(Vec2::splat(1.0 + 1e-5)).all())
}

#[test]
fn a_cube_cut_at_every_edge_unwraps_to_six_equal_squares_packed_without_overlap() {
    let mut mesh = cube();
    mesh.set_seams(edges(&mesh), true);
    assert_eq!(mesh.seams().len(), 12);
    let faces = all_faces(&mesh);
    uv::unwrap(&mut mesh, &faces, 0.02, 0).unwrap();
    let islands = uv::uv_islands(&mesh, &all_faces(&mesh), 0);
    assert_eq!(islands.len(), 6);
    let sides: Vec<f32> = (0..6).map(|f| square_side(&mesh, f)).collect();
    for side in &sides {
        assert!((side - sides[0]).abs() < 1e-3, "{sides:?}");
    }
    assert!(uv::overlapping_islands(&mesh, &all_faces(&mesh), 0).is_empty());
    assert!(!faces_overlap(&mesh));
    assert!(in_texture(&mesh));
    // Six squares in three rows of two fill the texture's height.
    assert!(sides[0] > 0.28, "{sides:?}");
    for stretch in uv::stretch(&mesh, &all_faces(&mesh), 0) {
        assert!(stretch.angle < 1e-3 && stretch.area < 1e-3, "{stretch:?}");
    }
}

#[test]
fn a_cube_cut_as_a_cross_unfolds_into_a_cross_of_six_squares() {
    let mut mesh = cube();
    let [top, bottom, front, back, left, right] =
        [Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z, -Vec3::X, Vec3::X].map(|n| face_facing(&mesh, n));
    // The cross's column is back, top, front, bottom; the arms hang off the top.
    let hinges: BTreeSet<[u32; 2]> = [
        (back, top),
        (top, front),
        (front, bottom),
        (top, left),
        (top, right),
    ]
    .into_iter()
    .map(|(a, b)| shared_edge(&mesh, a, b))
    .map(|[a, b]| [a.min(b), a.max(b)])
    .collect();
    let cuts: Vec<[u32; 2]> = edges(&mesh)
        .into_iter()
        .filter(|e| !hinges.contains(e))
        .collect();
    assert_eq!(cuts.len(), 7);
    mesh.set_seams(cuts, true);
    let faces = all_faces(&mesh);
    uv::unwrap(&mut mesh, &faces, 0.02, 0).unwrap();
    assert_eq!(uv::uv_islands(&mesh, &all_faces(&mesh), 0).len(), 1);
    let side = square_side(&mesh, top);
    for face in 0..6 {
        assert!((square_side(&mesh, face) - side).abs() < 1e-3);
    }
    assert!(!faces_overlap(&mesh), "the cross doesn't fold over itself");
    assert!(in_texture(&mesh));
    // The arms sit either side of the top, the column runs through it.
    let middle = |face: u32| face_uvs(&mesh, face).iter().sum::<Vec2>() / 4.0;
    let (t, l, r, f, b) = (
        middle(top),
        middle(left),
        middle(right),
        middle(front),
        middle(back),
    );
    assert!(((l + r) / 2.0).distance(t) < 1e-3);
    assert!(((f + b) / 2.0).distance(t) < 1e-3);
    assert!((l - t).dot(f - t).abs() < 1e-4, "arms across the column");
}

#[test]
fn pinned_vertices_keep_their_uvs_through_an_unwrap() {
    let mut mesh = cube();
    let top = face_facing(&mesh, Vec3::Y);
    mesh.set_seams(edges(&mesh), true);
    let corners = mesh.face_half_edges(top);
    let (a, b) = (corners[0], corners[2]);
    mesh.set_uv(a, 0, Vec2::new(0.1, 0.1));
    mesh.set_uv(b, 0, Vec2::new(0.4, 0.4));
    mesh.set_pins([mesh.half_edge(a).vertex, mesh.half_edge(b).vertex], true);
    uv::unwrap(&mut mesh, &[top], 0.02, 0).unwrap();
    assert!(mesh.uv(a, 0).abs_diff_eq(Vec2::new(0.1, 0.1), 1e-5));
    assert!(mesh.uv(b, 0).abs_diff_eq(Vec2::new(0.4, 0.4), 1e-5));
    // The face between them is a square with them at opposite corners.
    let side = square_side(&mesh, top);
    assert!((side * 2f32.sqrt() - 0.3 * 2f32.sqrt()).abs() < 1e-3);
    // Pack leaves a pinned island where it is.
    assert!(uv::pack_islands(&mut mesh, &[top], 0.02, true, 0).is_err());
}

#[test]
fn smart_uv_project_lays_a_cube_out_as_six_unstretched_squares() {
    let mut mesh = cube();
    let faces = all_faces(&mesh);
    uv::smart_project(&mut mesh, &faces, 66.0, 0.02, 0).unwrap();
    assert_eq!(uv::uv_islands(&mesh, &all_faces(&mesh), 0).len(), 6);
    for face in 0..6 {
        square_side(&mesh, face);
    }
    assert!(uv::overlapping_islands(&mesh, &all_faces(&mesh), 0).is_empty());
    assert!(in_texture(&mesh));
}

#[test]
fn smart_uv_project_flattens_a_cylinder_without_overlap_and_little_stretch() {
    let mut mesh = PrimitiveSettings::of(PrimitiveShape::Cylinder).build();
    let faces = all_faces(&mesh);
    uv::smart_project(&mut mesh, &faces, 66.0, 0.02, 0).unwrap();
    assert!(uv::overlapping_islands(&mesh, &faces, 0).is_empty());
    assert!(in_texture(&mesh));
    let stretch = uv::stretch(&mesh, &faces, 0);
    let worst = stretch
        .iter()
        .map(|s| s.angle.max(s.area))
        .fold(0.0, f32::max);
    assert!(worst < 0.1, "{worst}");
}

#[test]
fn cube_projection_puts_each_face_on_the_whole_texture() {
    let mut mesh = cube();
    let faces = all_faces(&mesh);
    uv::project(&mut mesh, &faces, Projection::Cube, 0).unwrap();
    for face in 0..6 {
        let side = square_side(&mesh, face);
        assert!((side - 1.0).abs() < 1e-4);
    }
    // Seen from outside, the front face's top left is the texture's.
    let front = face_facing(&mesh, Vec3::Z);
    for edge in mesh.face_half_edges(front) {
        let p = mesh.position(mesh.half_edge(edge).vertex);
        if p.x < 0.0 && p.y > 0.0 {
            assert!(mesh.uv(edge, 0).abs_diff_eq(Vec2::ZERO, 1e-5));
        }
    }
}

#[test]
fn cylinder_and_sphere_projections_wrap_once_round_without_a_face_across_the_texture() {
    for (shape, projection) in [
        (PrimitiveShape::Cylinder, Projection::Cylinder),
        (PrimitiveShape::UvSphere, Projection::Sphere),
    ] {
        let mut mesh = PrimitiveSettings::of(shape).build();
        let faces = all_faces(&mesh);
        uv::project(&mut mesh, &faces, projection, 0).unwrap();
        let mut lo: f32 = 1.0;
        let mut hi: f32 = 0.0;
        // A cylinder's caps go all the way round; the sides don't.
        let sides = faces.iter().filter(|&&f| mesh.face_normal(f).y.abs() < 0.9);
        for &face in sides {
            let us: Vec<f32> = face_uvs(&mesh, face).iter().map(|uv| uv.x).collect();
            let span =
                us.iter().fold(0.0f32, |m, &u| m.max(u)) - us.iter().fold(2.0f32, |m, &u| m.min(u));
            assert!(span < 0.5, "{shape:?} face {face} spans {span}");
            lo = us.iter().fold(lo, |m, &u| m.min(u));
            hi = us.iter().fold(hi, |m, &u| m.max(u));
        }
        assert!(lo < 0.05 && hi > 0.95, "{shape:?}: {lo}..{hi}");
        // The top is the top of the texture.
        let top = (0..mesh.half_edges().len() as u32)
            .max_by(|&a, &b| {
                mesh.position(mesh.half_edge(a).vertex)
                    .y
                    .total_cmp(&mesh.position(mesh.half_edge(b).vertex).y)
            })
            .unwrap();
        assert!(mesh.uv(top, 0).y < 0.05, "{shape:?}");
    }
}

#[test]
fn project_from_view_matches_what_the_view_sees() {
    let mut mesh = cube();
    use glam::camera::rh;
    let view: Mat4 = rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0)
        * rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, Vec3::Y);
    let front = face_facing(&mesh, Vec3::Z);
    uv::project(&mut mesh, &[front], Projection::View(view), 0).unwrap();
    // Seen straight on, the front face fills the texture the right way up.
    assert!((square_side(&mesh, front) - 1.0).abs() < 1e-4);
    for edge in mesh.face_half_edges(front) {
        let p = mesh.position(mesh.half_edge(edge).vertex);
        let expected = Vec2::new((p.x > 0.0) as u8 as f32, (p.y < 0.0) as u8 as f32);
        assert!(mesh.uv(edge, 0).abs_diff_eq(expected, 1e-4));
    }
}

#[test]
fn pack_turns_islands_upright_and_keeps_the_margin_between_them() {
    let mut mesh = cube();
    mesh.set_seams(edges(&mesh), true);
    let faces = all_faces(&mesh);
    uv::unwrap(&mut mesh, &faces, 0.0, 0).unwrap();
    // Turn one island 30° and make it long: Pack turns it back square.
    let top = face_facing(&mesh, Vec3::Y);
    let corners = mesh.face_half_edges(top);
    let turn = Affine2::from_angle(0.5) * Affine2::from_scale(Vec2::new(3.0, 1.0));
    uv::transform_uvs(&mut mesh, &corners, turn, 0);
    let margin = 0.05;
    let faces = all_faces(&mesh);
    uv::pack_islands(&mut mesh, &faces, margin, true, 0).unwrap();
    assert!(in_texture(&mesh));
    let boxes: Vec<(Vec2, Vec2)> = (0..6)
        .map(|f| {
            face_uvs(&mesh, f)
                .into_iter()
                .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(lo, hi), uv| {
                    (lo.min(uv), hi.max(uv))
                })
        })
        .collect();
    let (lo, hi) = boxes[top as usize];
    let size = hi - lo;
    assert!(size.x > size.y * 2.9, "upright and lying flat: {size}");
    square_corners(&mesh, top);
    for i in 0..6 {
        for j in i + 1..6 {
            let (a, b) = (boxes[i], boxes[j]);
            let gap_x = (b.0.x - a.1.x).max(a.0.x - b.1.x);
            let gap_y = (b.0.y - a.1.y).max(a.0.y - b.1.y);
            assert!(
                gap_x.max(gap_y) > margin * 0.99,
                "{i} and {j}: {gap_x} {gap_y}"
            );
        }
    }
}

#[test]
fn average_island_scale_gives_each_island_the_same_texels_per_stud() {
    let mut mesh = cube();
    mesh.set_seams(edges(&mesh), true);
    let faces = all_faces(&mesh);
    uv::unwrap(&mut mesh, &faces, 0.02, 0).unwrap();
    let corners = mesh.face_half_edges(0);
    uv::transform_uvs(
        &mut mesh,
        &corners,
        Affine2::from_scale(Vec2::splat(0.3)),
        0,
    );
    assert!(uv::stretch(&mesh, &[0, 1], 0)[0].area > 0.5);
    let faces = all_faces(&mesh);
    uv::average_island_scale(&mut mesh, &faces, 0).unwrap();
    for stretch in uv::stretch(&mesh, &all_faces(&mesh), 0) {
        assert!(stretch.area < 1e-3, "{stretch:?}");
    }
}

#[test]
fn stitch_joins_a_cut_and_align_lines_uvs_up() {
    let mut mesh = cube();
    mesh.set_seams(edges(&mesh), true);
    let faces = all_faces(&mesh);
    uv::unwrap(&mut mesh, &faces, 0.02, 0).unwrap();
    let (top, front) = (face_facing(&mesh, Vec3::Y), face_facing(&mesh, Vec3::Z));
    let [a, b] = shared_edge(&mesh, top, front);
    let corners: Vec<u32> = [top, front]
        .into_iter()
        .flat_map(|f| mesh.face_half_edges(f))
        .filter(|&e| [a, b].contains(&mesh.half_edge(e).vertex))
        .collect();
    assert_eq!(corners.len(), 4);
    assert_eq!(uv::stitch(&mut mesh, &corners, 0), 2);
    assert_eq!(uv::uv_islands(&mesh, &all_faces(&mesh), 0).len(), 5);
    // The uv vertex of a corner is every corner of its vertex with its uv.
    assert_eq!(uv::uv_vertex(&mesh, corners[0], 0).len(), 2);

    let picked = mesh.face_half_edges(top);
    uv::align(&mut mesh, &picked, AlignAxis::V, 0);
    let v = mesh.uv(picked[0], 0).y;
    assert!(picked.iter().all(|&e| (mesh.uv(e, 0).y - v).abs() < 1e-6));
}

#[test]
fn seams_and_pins_survive_an_operation_and_second_uvs_stay_on_untouched_faces() {
    let mut mesh = cube();
    let top = face_facing(&mesh, Vec3::Y);
    let bottom = face_facing(&mesh, -Vec3::Y);
    let bottom_edges: Vec<[u32; 2]> = mesh
        .face_half_edges(bottom)
        .into_iter()
        .map(|e| {
            let (a, b) = mesh.ends(e);
            [a, b]
        })
        .collect();
    mesh.set_seams(bottom_edges.clone(), true);
    let pinned = mesh.face_vertices(bottom)[0];
    mesh.set_pins([pinned], true);
    let faces = all_faces(&mesh);
    uv::project(&mut mesh, &faces, Projection::Cube, 1).unwrap();
    assert!(mesh.has_uv2());
    let bottom_uv2 = face_uvs2(&mesh, bottom);

    let mut selection = Selection {
        mode: SelectMode::Face,
        ..Selection::default()
    };
    selection.faces.insert(top);
    let op = MeshOp::Extrude {
        mode: ExtrudeMode::Region,
        distance: 1.0,
    };
    let (extruded, _) = apply(&mesh, &selection, &op).unwrap();
    assert_eq!(
        extruded.seams().len(),
        4,
        "the bottom's edges are still seams"
    );
    let bottom_now = face_facing(&extruded, -Vec3::Y);
    for e in extruded.face_half_edges(bottom_now) {
        let (a, b) = extruded.ends(e);
        assert!(extruded.is_seam(a, b));
    }
    assert_eq!(extruded.pins().len(), 1);
    assert!(extruded.has_uv2());
    assert_eq!(face_uvs2(&extruded, bottom_now), bottom_uv2);
}

fn face_uvs2(mesh: &EditMesh, face: u32) -> Vec<Vec2> {
    let mut uvs: Vec<Vec2> = mesh
        .face_half_edges(face)
        .into_iter()
        .map(|e| mesh.uv(e, 1))
        .collect();
    uvs.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    uvs
}

/// Each corner's position and uv in `channel`, rounded and sorted.
fn corner_uvs(mesh: &EditMesh, channel: usize) -> Vec<([i64; 3], [i64; 2])> {
    let mut corners: Vec<_> = (0..mesh.half_edges().len() as u32)
        .map(|e| {
            let p = mesh.position(mesh.half_edge(e).vertex);
            let uv = mesh.uv(e, channel);
            (
                (p * 1e4).round().as_i64vec3().to_array(),
                (uv * 1e4).round().as_i64vec2().to_array(),
            )
        })
        .collect();
    corners.sort_unstable();
    corners.dedup();
    corners
}

#[test]
fn unwrapped_uvs_save_in_the_glb_and_seams_pins_and_second_uvs_in_the_sidecar() {
    let mut doc = ModelDoc::primitive(&PrimitiveSettings::default());
    let mesh = doc.objects[0].mesh.as_mut().unwrap();
    mesh.set_seams(edges(mesh), true);
    let faces = all_faces(mesh);
    uv::unwrap(mesh, &faces, 0.02, 0).unwrap();
    uv::smart_project(mesh, &faces, 66.0, 0.05, 1).unwrap();
    mesh.set_pins([0], true);
    let before = mesh.clone();

    let glb = doc.write_glb();
    let json = glb_json(&glb);
    let attributes = &json["meshes"][0]["primitives"][0]["attributes"];
    assert!(attributes["TEXCOORD_0"].is_number());
    assert!(
        attributes["TEXCOORD_1"].is_number(),
        "the second uvs are saved"
    );

    // The .glb alone gives back the uvs (though no material has a texture).
    let folder = std::env::temp_dir();
    let plain = ModelDoc::open(&glb, &folder, ModelFormat::Gltf).unwrap();
    assert!(plain.objects[0].uvs);
    assert_eq!(
        corner_uvs(plain.objects[0].mesh.as_ref().unwrap(), 0),
        corner_uvs(&before, 0)
    );

    // The sidecar gives back the seams, the pin and the second uvs.
    let sidecar = doc.write_sidecar(&glb).expect("seams need a sidecar");
    let mut reopened = ModelDoc::open(&glb, &folder, ModelFormat::Gltf).unwrap();
    assert_eq!(reopened.read_sidecar(&sidecar, &glb), Ok(1));
    let mesh = reopened.objects[0].mesh.as_ref().unwrap();
    assert_eq!(mesh.seams(), before.seams());
    assert_eq!(mesh.pins(), before.pins());
    assert!(mesh.has_uv2());
    assert_eq!(corner_uvs(mesh, 1), corner_uvs(&before, 1));
}

/// The JSON chunk of a `.glb`.
fn glb_json(glb: &[u8]) -> serde_json::Value {
    let length = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&glb[20..20 + length]).unwrap()
}
