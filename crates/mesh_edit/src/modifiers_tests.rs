//! Phase 57: modifiers, the sidecar that keeps them, and Generate LODs.

use std::collections::BTreeSet;

use axcenity_assets::{ModelFormat, load_model};
use glam::Vec3;

use crate::{
    Axis, DeleteKind, EditMesh, MeshOp, ModelDoc, Modifier, ModifierKind, PrimitiveSettings,
    PrimitiveShape, Purpose, SelectMode, Selection, apply, check_manifold, clip, evaluate,
};

fn cube() -> EditMesh {
    PrimitiveSettings::of(PrimitiveShape::Cube).build()
}

fn every_face(mesh: &EditMesh) -> Selection {
    Selection {
        mode: SelectMode::Face,
        faces: (0..mesh.face_count() as u32).collect(),
        ..Selection::default()
    }
}

/// The faces of `mesh` whose middles are on the −X side.
fn faces_left(mesh: &EditMesh) -> Selection {
    Selection {
        mode: SelectMode::Face,
        faces: (0..mesh.face_count() as u32)
            .filter(|&f| mesh.face_center(f).x < -1e-4)
            .collect(),
        ..Selection::default()
    }
}

/// A head-sized ball cut in half down the middle: the +X half, open where
/// it was cut, shaded smooth.
fn half_head() -> EditMesh {
    let mut settings = PrimitiveSettings::of(PrimitiveShape::UvSphere);
    settings.segments = 16;
    settings.rings = 8;
    let ball = settings.build();
    let (half, _) = apply(
        &ball,
        &faces_left(&ball),
        &MeshOp::Delete(DeleteKind::Faces),
    )
    .unwrap();
    let (half, _) = apply(&half, &every_face(&half), &MeshOp::ShadeSmooth).unwrap();
    half
}

fn bounds(mesh: &EditMesh) -> (Vec3, Vec3) {
    let first = mesh.position(0);
    mesh.positions()
        .iter()
        .fold((first, first), |(min, max), &p| (min.min(p), max.max(p)))
}

/// The largest angle, in degrees, between two faces sharing an edge.
fn sharpest(mesh: &EditMesh) -> f32 {
    mesh.edges()
        .into_iter()
        .filter_map(|edge| {
            let twin = mesh.half_edge(edge).twin?;
            let a = mesh.face_normal(mesh.half_edge(edge).face);
            let b = mesh.face_normal(mesh.half_edge(twin).face);
            Some(a.angle_between(b).to_degrees())
        })
        .fold(0.0, f32::max)
}

fn triangles(mesh: &EditMesh) -> usize {
    (0..mesh.face_count() as u32)
        .map(|f| mesh.triangles(f).len())
        .sum()
}

fn mirror(clip: bool) -> Modifier {
    Modifier::new(ModifierKind::Mirror {
        axis: Axis::X,
        clip,
        merge: true,
        merge_distance: 0.001,
    })
}

fn subdivision(viewport: u32, export: u32) -> Modifier {
    Modifier::new(ModifierKind::Subdivision { viewport, export })
}

/// A doc of one object named `name` with `mesh` and `modifiers`.
fn doc_of(name: &str, mesh: EditMesh, modifiers: Vec<Modifier>) -> ModelDoc {
    let mut doc = ModelDoc::primitive(&PrimitiveSettings::default());
    doc.objects[0].name = name.to_owned();
    doc.objects[0].mesh = Some(mesh);
    doc.objects[0].modifiers = modifiers;
    doc
}

#[test]
fn a_half_head_with_mirror_and_subdivision_saves_whole_and_smooth_and_reopens_as_the_half() {
    let half = half_head();
    assert!(check_manifold(&half, true).is_err(), "the half is open");
    let (min, _) = bounds(&half);
    assert!(min.x > -1e-4);
    let stack = vec![mirror(true), subdivision(1, 2)];
    let doc = doc_of("Head", half.clone(), stack.clone());

    let glb = doc.write_glb();
    let sidecar = doc.write_sidecar(&glb).expect("the stack needs a sidecar");

    // What a game loads is the whole head, smoothed.
    let folder = std::env::temp_dir();
    let export = doc.objects[0].evaluated(Purpose::Export).unwrap();
    let saved = ModelDoc::open(&glb, &folder, ModelFormat::Gltf).unwrap();
    let whole = saved.objects[0].mesh.as_ref().unwrap();
    assert_eq!(whole.face_count(), triangles(&export));
    check_manifold(whole, true).expect("mirrored and merged into one closed surface");
    let (min, max) = bounds(whole);
    assert!((min.x + max.x).abs() < 1e-3, "both sides: {min} {max}");
    assert!(
        sharpest(whole) < sharpest(&half) / 3.0,
        "smoother: {} against {}",
        sharpest(whole),
        sharpest(&half)
    );
    assert!(saved.objects[0].modifiers.is_empty());

    // Reopened with the sidecar: the half, and the stack, still editable.
    let mut reopened = ModelDoc::open(&glb, &folder, ModelFormat::Gltf).unwrap();
    assert_eq!(reopened.read_sidecar(&sidecar, &glb), Ok(1));
    let object = &reopened.objects[0];
    assert_eq!(object.mesh.as_ref(), Some(&half));
    assert_eq!(object.modifiers, stack);
    let viewport = object.evaluated(Purpose::Viewport).unwrap();
    assert_eq!(
        viewport.face_count() * 4,
        export.face_count(),
        "one level in the view"
    );

    // Raising the top of the half raises both sides.
    let mesh = reopened.objects[0].mesh.as_mut().unwrap();
    let top = (0..mesh.vertex_count() as u32)
        .max_by(|&a, &b| mesh.position(a).y.total_cmp(&mesh.position(b).y))
        .unwrap();
    mesh.transform(&[top], glam::Mat4::from_translation(Vec3::Y * 0.5));
    let edited = reopened.objects[0].evaluated(Purpose::Export).unwrap();
    check_manifold(&edited, true).unwrap();
    let (min, max) = bounds(&edited);
    let (_, before) = bounds(&export);
    assert!(max.y > before.y + 0.1 && (min.x + max.x).abs() < 1e-3);
}

#[test]
fn a_sidecar_isnt_used_once_the_glb_was_changed_elsewhere() {
    let doc = doc_of("Head", half_head(), vec![mirror(false)]);
    let glb = doc.write_glb();
    let sidecar = doc.write_sidecar(&glb).unwrap();
    let other = doc_of("Head", cube(), vec![mirror(false)]).write_glb();
    let mut reopened = ModelDoc::open(&other, &std::env::temp_dir(), ModelFormat::Gltf).unwrap();
    assert!(reopened.read_sidecar(&sidecar, &other).is_err());
    assert!(reopened.objects[0].modifiers.is_empty());
    // Without modifiers there's nothing for a sidecar to hold.
    assert!(
        doc_of("Box", cube(), Vec::new())
            .write_sidecar(&glb)
            .is_none()
    );
}

#[test]
fn every_modifier_keeps_its_settings_through_the_sidecar() {
    let stack: Vec<Modifier> = ModifierKind::defaults()
        .into_iter()
        .map(Modifier::new)
        .collect();
    let mut hidden = stack.clone();
    hidden[2].show = false;
    let doc = doc_of("Box", cube(), hidden.clone());
    let glb = doc.write_glb();
    let sidecar = doc.write_sidecar(&glb).unwrap();
    let mut reopened = ModelDoc::open(&glb, &std::env::temp_dir(), ModelFormat::Gltf).unwrap();
    reopened.read_sidecar(&sidecar, &glb).unwrap();
    assert_eq!(reopened.objects[0].modifiers, hidden);
    assert_eq!(reopened.objects[0].mesh.as_ref(), Some(&cube()));
}

#[test]
fn hidden_modifiers_are_skipped_and_one_that_cant_run_is_passed_over() {
    let mut stack = vec![subdivision(1, 1), subdivision(7, 7)];
    stack[0].show = false;
    let evaluated = evaluate(&cube(), &stack, Purpose::Viewport);
    // The second runs 6 levels at most.
    assert!(evaluated.errors.is_empty(), "{:?}", evaluated.errors);
    assert_eq!(evaluated.mesh.face_count(), 6 * 4usize.pow(6));

    let mut settings = PrimitiveSettings::of(PrimitiveShape::UvSphere);
    settings.segments = 32;
    settings.rings = 16;
    let ball = settings.build();
    let stack = vec![subdivision(6, 6), Modifier::new(ModifierKind::Triangulate)];
    let evaluated = evaluate(&ball, &stack, Purpose::Viewport);
    assert_eq!(evaluated.errors.len(), 1, "too many faces");
    assert_eq!(evaluated.errors[0].0, 0);
    assert_eq!(
        evaluated.mesh.face_count(),
        triangles(&ball),
        "the rest still ran"
    );
}

#[test]
fn subdivision_rounds_a_cube_into_a_closed_ball() {
    let cube = cube();
    let round = ModifierKind::Subdivision {
        viewport: 3,
        export: 3,
    }
    .run(&cube, Purpose::Viewport)
    .unwrap();
    check_manifold(&round, true).unwrap();
    assert_eq!(round.face_count(), 6 * 64);
    // The corners come in towards the middle; the faces' middles stay out.
    let corner = round
        .positions()
        .iter()
        .map(|p| p.length())
        .fold(0.0, f32::max);
    assert!(corner < 3f32.sqrt() * 0.75, "{corner}");
    assert!(sharpest(&round) < 15.0);
}

#[test]
fn subdivision_keeps_an_open_border_on_its_plane() {
    let plane = PrimitiveSettings::of(PrimitiveShape::Plane).build();
    let smooth = ModifierKind::Subdivision {
        viewport: 2,
        export: 2,
    }
    .run(&plane, Purpose::Viewport)
    .unwrap();
    check_manifold(&smooth, false).unwrap();
    assert!(smooth.positions().iter().all(|p| p.y.abs() < 1e-6));
    assert_eq!(smooth.face_count(), 16);
}

#[test]
fn mirror_without_merge_leaves_two_halves_and_clipping_keeps_the_seam_on_the_plane() {
    let half = half_head();
    let apart = ModifierKind::Mirror {
        axis: Axis::X,
        clip: false,
        merge: false,
        merge_distance: 0.001,
    }
    .run(&half, Purpose::Viewport)
    .unwrap();
    assert_eq!(apart.vertex_count(), half.vertex_count() * 2);
    check_manifold(&apart, false).unwrap();

    let stack = vec![mirror(true)];
    let on_plane = Vec3::new(0.0, 0.5, 0.5);
    assert_eq!(clip(&stack, on_plane, Vec3::new(0.3, 1.0, 0.5)).x, 0.0);
    assert_eq!(
        clip(&stack, Vec3::new(0.2, 0.0, 0.0), Vec3::new(-0.4, 1.0, 0.0)),
        Vec3::new(0.0, 1.0, 0.0),
        "can't cross"
    );
    let free = Vec3::new(0.6, 1.0, 0.0);
    assert_eq!(clip(&stack, Vec3::new(0.2, 0.0, 0.0), free), free);
    assert_eq!(clip(&[mirror(false)], on_plane, free), free);
}

#[test]
fn an_array_of_cubes_with_merge_is_one_closed_bar() {
    let cube = cube();
    let array = |merge| ModifierKind::Array {
        count: 3,
        offset: [1.0, 0.0, 0.0],
        merge,
        merge_distance: 0.001,
    };
    let apart = array(false).run(&cube, Purpose::Export).unwrap();
    assert_eq!(apart.face_count(), 18);
    let (min, max) = bounds(&apart);
    assert!((max.x - min.x - 6.0).abs() < 1e-5);
    let bar = array(true).run(&cube, Purpose::Export).unwrap();
    check_manifold(&bar, true).unwrap();
    assert_eq!(bar.vertex_count(), 16, "touching corners are shared");
    assert_eq!(bar.face_count(), 14, "the faces between copies go");
}

#[test]
fn solidify_makes_a_plane_a_closed_slab_and_a_cube_a_hollow_shell() {
    let plane = PrimitiveSettings::of(PrimitiveShape::Plane).build();
    let slab = ModifierKind::Solidify {
        thickness: 0.2,
        offset: -1.0,
    }
    .run(&plane, Purpose::Export)
    .unwrap();
    check_manifold(&slab, true).unwrap();
    assert_eq!(slab.face_count(), 6);
    let (min, max) = bounds(&slab);
    assert!((min.y + 0.2).abs() < 1e-5 && max.y.abs() < 1e-5);

    let shell = ModifierKind::Solidify {
        thickness: 0.1,
        offset: -1.0,
    }
    .run(&cube(), Purpose::Export)
    .unwrap();
    check_manifold(&shell, true).unwrap();
    assert_eq!(shell.face_count(), 12);
}

#[test]
fn bevel_by_angle_rounds_a_cubes_edges_but_not_a_plane() {
    let bevel = ModifierKind::Bevel {
        width: 0.1,
        segments: 2,
        angle: 30.0,
    };
    let rounded = bevel.run(&cube(), Purpose::Export).unwrap();
    check_manifold(&rounded, true).unwrap();
    assert!(rounded.face_count() > 6);
    assert!(sharpest(&rounded) < 60.0);
    let plane = PrimitiveSettings::of(PrimitiveShape::Plane).build();
    assert_eq!(bevel.run(&plane, Purpose::Export).unwrap(), plane);
}

#[test]
fn decimate_keeps_about_its_share_of_triangles_and_triangulate_keeps_the_shape() {
    let mut settings = PrimitiveSettings::of(PrimitiveShape::UvSphere);
    settings.segments = 32;
    settings.rings = 16;
    let ball = settings.build();
    let all = triangles(&ball);
    let half = ModifierKind::Decimate { ratio: 0.5 }
        .run(&ball, Purpose::Export)
        .unwrap();
    let kept = triangles(&half);
    assert!(
        kept <= all / 2 + 2 && kept > all / 4,
        "{kept} of {all} triangles"
    );
    assert!((0..half.face_count() as u32).all(|f| half.face(f).smooth));

    let cut = ModifierKind::Triangulate
        .run(&ball, Purpose::Export)
        .unwrap();
    check_manifold(&cut, true).unwrap();
    assert_eq!(cut.face_count(), all);
    assert_eq!(cut.positions(), ball.positions());
    let corners = |mesh: &EditMesh| {
        let mut normals: Vec<[u32; 3]> = mesh
            .half_edges()
            .iter()
            .map(|e| e.normal.to_array().map(f32::to_bits))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        normals.sort_unstable();
        normals
    };
    assert_eq!(corners(&cut), corners(&ball), "shading kept");
}

#[test]
fn generated_lods_are_the_levels_the_renderer_draws() {
    let mut settings = PrimitiveSettings::of(PrimitiveShape::UvSphere);
    settings.segments = 32;
    settings.rings = 16;
    let mut doc = doc_of("Rock", settings.build(), vec![subdivision(0, 1)]);
    assert_eq!(doc.generate_lods(0), Some(0));
    let names: Vec<&str> = doc.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["Rock", "Rock_LOD1", "Rock_LOD2"]);
    assert!(doc.is_lod_copy(1) && doc.is_lod_copy(2) && !doc.is_lod_copy(0));
    // Again: the copies are replaced, not added to.
    assert_eq!(doc.generate_lods(0), Some(0));
    assert_eq!(doc.objects.len(), 3);
    assert_eq!(doc.generate_lods(1), None, "a copy has no copies");

    let counts: Vec<usize> = doc
        .objects
        .iter()
        .map(|o| triangles(&o.evaluated(Purpose::Export).unwrap()))
        .collect();
    assert!(counts[1] < counts[0] && counts[2] < counts[1], "{counts:?}");
    // The Studio draws the full one only.
    let drawn = doc.to_model().unwrap();
    assert_eq!(drawn.primitives.len(), 1);
    let viewport = doc.objects[0].evaluated(Purpose::Viewport).unwrap();
    assert_eq!(drawn.primitives[0].indices.len() / 3, triangles(&viewport));

    let model = load_model(&doc.write_glb(), &std::env::temp_dir(), ModelFormat::Gltf).unwrap();
    assert_eq!(model.primitives.len(), 1);
    let full = &model.primitives[0];
    assert_eq!(full.indices.len() / 3, counts[0]);
    let levels: Vec<usize> = full.lods.iter().map(|lod| lod.indices.len() / 3).collect();
    assert_eq!(
        levels,
        counts[1..],
        "the file's own copies, not simplified ones"
    );
}

/// Phase 90: a sidecar with fields this Axcenity no longer reads (a sculpt
/// or a retopology reference from an older or newer editor) still opens
/// with the same shape, and saving again leaves those fields out.
#[test]
fn a_sidecar_with_fields_no_longer_read_still_opens_and_saves_without_them() {
    let doc = doc_of("Head", half_head(), vec![mirror(false)]);
    let glb = doc.write_glb();
    let sidecar = doc.write_sidecar(&glb).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&sidecar).unwrap();
    json["objects"][0]["sculpt"] = serde_json::json!({"mask": [0.5, 1.0], "levels": 2});
    json["retopo"] = serde_json::json!({"reference": 0});
    let old = serde_json::to_vec(&json).unwrap();

    let mut reopened = ModelDoc::open(&glb, &std::env::temp_dir(), ModelFormat::Gltf).unwrap();
    assert_eq!(reopened.read_sidecar(&old, &glb), Ok(1));
    let mesh = reopened.objects[0].mesh.as_ref().unwrap();
    assert_eq!(
        mesh.positions(),
        doc.objects[0].mesh.as_ref().unwrap().positions()
    );
    assert_eq!(reopened.objects[0].modifiers, doc.objects[0].modifiers);

    let saved = reopened.write_sidecar(&reopened.write_glb()).unwrap();
    let text = String::from_utf8(saved).unwrap();
    assert!(
        !text.contains("sculpt") && !text.contains("retopo"),
        "{text}"
    );
}
