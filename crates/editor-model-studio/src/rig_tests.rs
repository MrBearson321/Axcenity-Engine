//! Phase 62: rigging in the Model Studio. A modeled humanoid given the
//! Humanoid template and automatic weights bending cleanly at the elbows
//! and knees in Pose test; bones added, extruded, moved, renamed,
//! parented, rolled, mirrored and deleted as undo steps; a weight stroke
//! as one step; and snapshots of Pose and Weights mode.

use axcenity_assets::Model;
use axcenity_mesh_edit::{Armature, DocObject, ModelDoc, PrimitiveSettings};
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;
use eframe::egui;

use super::tests::studio_with;
use super::*;
use crate::screen::Screen;

/// How tall the humanoid is, in studs.
pub const HEIGHT: f32 = 5.0;

/// A model of one object, `Body`: a humanoid standing in a T-pose,
/// facing +Z, its left at +X (modeled round where the template's bones
/// go, as an artist would model to Mixamo's proportions).
pub fn humanoid_doc() -> ModelDoc {
    let guide = Armature::humanoid(Vec3::new(-2.6, 0.0, -0.5), Vec3::new(2.6, HEIGHT, 0.5));
    let body = guide.stand_in_body(HEIGHT / 90.0).unwrap();
    let mut model = ModelDoc::primitive(&PrimitiveSettings::default());
    model.objects = vec![DocObject {
        name: "Body".to_owned(),
        mesh: Some(body),
        ..model.objects[0].clone()
    }];
    model
}

/// The humanoid in the Studio, given the Humanoid rig and automatic
/// weights in Rig mode, each an undo step.
pub fn rigged(studio: &mut ModelStudio) {
    let doc = studio.doc.as_mut().unwrap();
    doc.set_mode(Mode::Armature);
    assert_eq!(doc.mode, Mode::Armature);
    assert!(studio.add_humanoid());
    assert_eq!(
        studio.doc.as_ref().unwrap().undo_label(),
        Some("Add Humanoid Rig")
    );
    assert!(studio.weigh(false));
    let doc = studio.doc.as_ref().unwrap();
    assert_eq!(doc.undo_label(), Some("Automatic Weights"));
    assert!(doc.model.objects[0].skin.is_some());
}

pub fn bone(studio: &ModelStudio, name: &str) -> usize {
    let armature = studio
        .doc
        .as_ref()
        .unwrap()
        .model
        .armature
        .as_ref()
        .unwrap();
    armature.bone(&format!("mixamorig:{name}")).unwrap()
}

/// The drawn model's corners in the file's space (undoing the fit).
pub fn corners(model: &Model) -> Vec<Vec3> {
    let size = Vec3::from(model.size);
    let scale = Vec3::select(size.cmpgt(Vec3::ZERO), size, Vec3::ONE);
    let center = Vec3::from(model.center);
    model
        .primitives
        .iter()
        .flat_map(|p| {
            p.positions
                .iter()
                .map(move |&c| Vec3::from(c) * scale + center)
        })
        .collect()
}

/// The drawn model's triangles, by corner number across its primitives.
fn triangles(model: &Model) -> Vec<[usize; 3]> {
    let mut first = 0;
    let mut out = Vec::new();
    for primitive in &model.primitives {
        for t in primitive.indices.as_chunks::<3>().0 {
            out.push(t.map(|k| first + k as usize));
        }
        first += primitive.positions.len();
    }
    out
}

/// The share of triangles turned over from `rest` to `posed`, and the most
/// any edge stretched.
fn bend_quality(rest: &[Vec3], posed: &[Vec3], triangles: &[[usize; 3]]) -> (f32, f32) {
    let (mut flipped, mut stretch) = (0, 0.0f32);
    for &[a, b, c] in triangles {
        let before = (rest[b] - rest[a]).cross(rest[c] - rest[a]);
        let after = (posed[b] - posed[a]).cross(posed[c] - posed[a]);
        if before.dot(after) < 0.0 {
            flipped += 1;
        }
        for (u, w) in [(a, b), (b, c), (c, a)] {
            stretch =
                stretch.max(posed[u].distance(posed[w]) / rest[u].distance(rest[w]).max(1e-6));
        }
    }
    (flipped as f32 / triangles.len().max(1) as f32, stretch)
}

fn near_segment(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
    p.distance(a + (b - a) * t)
}

#[test]
fn a_modeled_humanoid_with_the_template_rig_and_automatic_weights_bends_cleanly_at_the_elbows_and_knees_in_pose_test()
 {
    let mut studio = studio_with(StudioDoc::new(String::new(), humanoid_doc(), false));
    rigged(&mut studio);
    let doc = studio.doc.as_mut().unwrap();
    doc.set_mode(Mode::Pose);
    assert_eq!(doc.mode, Mode::Pose);
    let saved = doc.model.write_glb();
    let steps = doc.undo_label().map(str::to_owned);
    let armature = doc.model.armature.clone().unwrap();
    let rest = studio.rig_model().unwrap();
    let rest_corners = corners(&rest);
    let triangles = triangles(&rest);

    // Each elbow and knee bent a right angle in turn.
    for (joint, child, degrees) in [
        ("LeftForeArm", "LeftHand", Vec3::new(-90.0, 0.0, 0.0)),
        ("RightForeArm", "RightHand", Vec3::new(-90.0, 0.0, 0.0)),
        ("LeftLeg", "LeftFoot", Vec3::new(90.0, 0.0, 0.0)),
        ("RightLeg", "RightFoot", Vec3::new(90.0, 0.0, 0.0)),
    ] {
        studio.reset_pose();
        let b = bone(&studio, joint);
        studio.set_pose(b, degrees);
        let drawn = studio.rig_model().unwrap();
        let posed = corners(&drawn);
        assert_eq!(posed.len(), rest_corners.len());
        let (flipped, stretch) = bend_quality(&rest_corners, &posed, &triangles);
        assert!(
            flipped < 0.01,
            "{joint}: {flipped} of the triangles turned over"
        );
        assert!(stretch < 1.8, "{joint}: an edge stretched {stretch} times");

        let matrix = armature.skin_matrices(&studio.test_pose().unwrap())[b];
        let (head, tail) = (
            armature.bones[b].head,
            armature.bones[bone(&studio, child)].tail(),
        );
        let upper = armature.bones[b]
            .parent
            .map(|p| armature.bones[p].head)
            .unwrap();
        let mirrored = |name: &str| {
            let name = axcenity_mesh_edit::rig::mirror_name(name).unwrap();
            bone(&studio, &name)
        };
        let (other_head, other_tail) = (
            armature.bones[mirrored(joint)].head,
            armature.bones[mirrored(child)].tail(),
        );
        let along = |p: Vec3| (p - head).dot(tail - head) / (tail - head).length_squared();
        let (mut followed, mut stayed) = (0, 0);
        for (k, &p) in rest_corners.iter().enumerate() {
            let limb = near_segment(p, head, tail);
            let on_limb = limb < near_segment(p, upper, head)
                && limb < near_segment(p, other_head, other_tail)
                && limb < 0.08 * HEIGHT;
            // The far half of the forearm (or shin) and the hand (or foot)
            // turn with the bone, whole.
            if on_limb && along(p) > 0.5 && along(p) < 1.0 {
                let gap = posed[k].distance(matrix.transform_point3(p));
                assert!(gap < 0.01 * HEIGHT, "{joint}: corner {k} lags {gap}");
                followed += 1;
            }
            // Away from the joint, nothing moves.
            if along(p) < -1.2 || limb > 0.3 * HEIGHT {
                let gap = posed[k].distance(p);
                assert!(
                    gap < 0.005 * HEIGHT,
                    "{joint}: corner {k} far off moved {gap}"
                );
                stayed += 1;
            }
        }
        assert!(
            followed > 50 && stayed > 1000,
            "{joint}: {followed} {stayed}"
        );
    }

    // Pose test isn't an undo step and isn't saved.
    let doc = studio.doc.as_ref().unwrap();
    assert_eq!(doc.undo_label().map(str::to_owned), steps);
    assert_eq!(doc.model.write_glb(), saved);
}

/// Looking at the humanoid's front from 9 studs away, at chest height.
pub fn front_view() -> Screen {
    let camera = Camera {
        position: Vec3::new(0.0, 3.6, 9.0),
        look_at: Vec3::new(0.0, 3.6, 0.0),
        fov_y_degrees: 50.0,
    };
    Screen::new(
        camera,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
    )
}

#[test]
fn bones_are_added_extruded_moved_renamed_parented_rolled_mirrored_and_deleted_as_undo_steps() {
    let mut studio = studio_with(StudioDoc::new(String::new(), humanoid_doc(), false));
    studio.doc.as_mut().unwrap().set_mode(Mode::Armature);
    studio.moves.cursor = Vec3::new(0.6, 3.0, 0.0);
    let first = studio.add_bone().unwrap();
    let label = |studio: &ModelStudio| studio.doc.as_ref().unwrap().undo_label().map(str::to_owned);
    assert_eq!(label(&studio).as_deref(), Some("Add Bone"));
    let arm = |studio: &ModelStudio| studio.doc.as_ref().unwrap().model.armature.clone().unwrap();
    assert!(
        arm(&studio).bones[first]
            .head
            .abs_diff_eq(Vec3::new(0.6, 3.0, 0.0), 1e-5)
    );

    // E in the view: a child from the tail, whose tail follows the mouse
    // until a click keeps it, all one step.
    let screen = front_view();
    let tail = arm(&studio).bones[first].tail();
    let at = screen.point(tail).unwrap();
    let made = studio.extrude_bone(Some(at)).unwrap();
    studio.follow_bone_move(&screen, at + egui::vec2(60.0, 0.0));
    studio.finish_bone_move();
    assert_eq!(label(&studio).as_deref(), Some("Extrude Bone"));
    let extruded = arm(&studio).bones[made].clone();
    assert_eq!(extruded.parent, Some(first));
    assert!(extruded.head.abs_diff_eq(tail, 1e-5));
    assert!(extruded.tail().x > tail.x + 0.3, "{}", extruded.tail());
    studio.doc.as_mut().unwrap().undo();
    assert_eq!(
        arm(&studio).bones.len(),
        1,
        "one undo takes the extrude back"
    );
    studio.doc.as_mut().unwrap().redo();

    // A click picks a bone by its tail; G moves the tail; Esc puts it back.
    let (picked, part) = studio
        .pick_bone(&screen, screen.point(extruded.tail()).unwrap())
        .unwrap();
    assert_eq!((picked, part), (made, rig::BonePart::Tail));
    studio.rig.bone = Some(picked);
    studio.rig.part = part;
    studio.begin_bone_move(at, "Move Bone", None);
    studio.follow_bone_move(&screen, at + egui::vec2(0.0, -80.0));
    studio.cancel_bone_move();
    assert_eq!(arm(&studio).bones[made], extruded);

    assert!(studio.rename_bone("forearm.L"));
    assert_eq!(arm(&studio).bones[made].name, "forearm.L");
    studio.rig.bone = Some(first);
    assert!(studio.rename_bone("upper_arm.L"));
    studio.set_roll(30.0);
    studio.set_roll(45.0);
    studio.end_roll();
    assert_eq!(label(&studio).as_deref(), Some("Roll Bone"));
    assert!((arm(&studio).bones[first].roll().to_degrees() - 45.0).abs() < 0.01);
    assert_eq!(studio.symmetrize_bones(), 2);
    let mirrored = arm(&studio);
    let right = mirrored.bone("upper_arm.R").unwrap();
    assert_eq!(
        mirrored.bones[mirrored.bone("forearm.R").unwrap()].parent,
        Some(right)
    );
    assert!(
        mirrored.bones[right]
            .head
            .abs_diff_eq(Vec3::new(-0.6, 3.0, 0.0), 1e-5)
    );
    // Parenting can't put a bone inside itself.
    studio.rig.bone = Some(first);
    assert!(!studio.parent_bone(Some(made)));
    assert!(studio.parent_bone(Some(right)));
    assert_eq!(arm(&studio).bones[first].parent, Some(right));
    studio.rig.bone = Some(made);
    assert!(studio.delete_bone());
    assert_eq!(label(&studio).as_deref(), Some("Delete Bone"));
    assert!(arm(&studio).bone("forearm.L").is_none());

    // Each was one step: undo goes back through them all.
    let doc = studio.doc.as_mut().unwrap();
    let mut steps = Vec::new();
    while let Some(step) = doc.undo_label().map(str::to_owned) {
        steps.push(step);
        doc.undo();
    }
    assert_eq!(
        steps,
        [
            "Delete Bone",
            "Parent Bone",
            "Symmetrize",
            "Roll Bone",
            "Rename Bone",
            "Rename Bone",
            "Extrude Bone",
            "Add Bone"
        ]
    );
    assert!(
        doc.model
            .armature
            .as_ref()
            .is_none_or(|a| a.bones.is_empty())
    );
}

#[test]
fn a_weight_stroke_on_the_shoulder_is_one_undo_step_and_shows_in_weights_mode() {
    let mut studio = studio_with(StudioDoc::new(String::new(), humanoid_doc(), false));
    rigged(&mut studio);
    studio.doc.as_mut().unwrap().set_mode(Mode::Weights);
    let shoulder = bone(&studio, "LeftShoulder");
    studio.rig.bone = Some(shoulder);
    studio.rig.brush.radius = 30.0;
    studio.rig.brush.strength = 1.0;
    let arm = studio.doc.as_ref().unwrap().model.armature.clone().unwrap();
    let screen = front_view();
    let (from, to) = (arm.bones[shoulder].head, arm.bones[shoulder].tail());
    let points: Vec<egui::Pos2> = (0..=8)
        .filter_map(|k| screen.point(from.lerp(to, k as f32 / 8.0) + Vec3::Z * 0.3))
        .collect();
    let weight_near = |studio: &ModelStudio| {
        let doc = studio.doc.as_ref().unwrap();
        let skin = doc.model.objects[0].skin.as_ref().unwrap();
        let mesh = doc.model.objects[0].mesh.as_ref().unwrap();
        (0..mesh.vertex_count())
            .filter(|&v| near_segment(mesh.position(v as u32), from, to) < 0.25)
            .map(|v| skin.weight(v, shoulder))
            .sum::<f32>()
    };
    let before = weight_near(&studio);
    let drawn_before = studio.rig_model_key();
    assert!(studio.weight_stroke(&screen, &points));
    assert!(
        weight_near(&studio) > before + 1.0,
        "{before} -> {}",
        weight_near(&studio)
    );
    assert_eq!(
        studio.doc.as_ref().unwrap().undo_label(),
        Some("Weight Stroke")
    );
    assert_ne!(
        studio.rig_model_key(),
        drawn_before,
        "the view draws the new weights"
    );
    // Drawn with the weights as colours: red (u near 1) where it's all the
    // shoulder's.
    let drawn = studio.rig_model().unwrap();
    let ramp = &drawn.primitives[0].material;
    assert_eq!(ramp.name, "Weights");
    assert!(drawn.primitives[0].uvs.iter().any(|uv| uv[0] > 0.9));
    studio.doc.as_mut().unwrap().undo();
    assert!((weight_near(&studio) - before).abs() < 1e-3);
}

/// Draws the tab as it is, saved as `name` in `folder`.
fn snapshot(studio: &mut ModelStudio, folder: &std::path::Path, name: &str) {
    crate::snapshot::snapshot_rendered(
        &folder.join(name),
        egui::vec2(1100.0, 700.0),
        vec![Vec::new(); 3],
        |ui, render| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = studio.show(ui, Some(render), &[], crate::gizmo::Snap::default(), true);
            });
        },
    );
}

#[test]
fn snapshot_of_pose_test() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let mut studio = studio_with(StudioDoc::new(String::new(), humanoid_doc(), false));
    rigged(&mut studio);
    studio.doc.as_mut().unwrap().set_mode(Mode::Pose);
    for (name, degrees) in [
        ("LeftForeArm", Vec3::new(-90.0, 0.0, 0.0)),
        ("RightForeArm", Vec3::new(-60.0, 0.0, 0.0)),
        ("LeftLeg", Vec3::new(90.0, 0.0, 0.0)),
        ("RightUpLeg", Vec3::new(-40.0, 0.0, 0.0)),
        ("RightLeg", Vec3::new(60.0, 0.0, 0.0)),
    ] {
        let b = bone(&studio, name);
        studio.set_pose(b, degrees);
    }
    studio.rig.bone = Some(bone(&studio, "LeftForeArm"));
    studio.view.look_from(30.0, 10.0, 1.3);
    snapshot(&mut studio, &folder, "model-studio-pose.png");
}

#[test]
fn snapshot_of_weights_mode() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let mut studio = studio_with(StudioDoc::new(String::new(), humanoid_doc(), false));
    rigged(&mut studio);
    studio.doc.as_mut().unwrap().set_mode(Mode::Weights);
    studio.rig.bone = Some(bone(&studio, "LeftArm"));
    studio.view.look_from(0.0, 5.0, 1.0);
    snapshot(&mut studio, &folder, "model-studio-weights.png");
}
