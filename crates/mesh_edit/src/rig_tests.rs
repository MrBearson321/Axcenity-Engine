//! Phase 62: rigging. The Humanoid template's bones as Mixamo's, bone
//! heat on a stand-in body bending cleanly at the elbows and knees, the
//! weight tools, armature edits, and a skinned `.glb` saved and read back.

use std::path::Path;

use axcenity_assets::{ModelFormat, load_model};
use glam::{Quat, Vec3};

use crate::half_edge::EditMesh;
use crate::model_doc::{DocObject, ModelDoc};
use crate::paint::BrushFalloff;
use crate::rig::{
    ArmBone, Armature, ROOT_NAME, Skin, WeightBrush, WeightPainter, WeightTool, automatic_weights,
    mirror_name, mirror_weights, vertex_skinning, weights_by_distance,
};

const HEIGHT: f32 = 5.0;

/// The template fitted 5 studs tall, and a body round it.
pub(crate) fn humanoid() -> (Armature, EditMesh) {
    let arm = Armature::humanoid(Vec3::new(-2.6, 0.0, -0.5), Vec3::new(2.6, HEIGHT, 0.5));
    let body = arm.stand_in_body(HEIGHT / 90.0).unwrap();
    (arm, body)
}

fn bone(arm: &Armature, name: &str) -> usize {
    arm.bone(&format!("mixamorig:{name}")).unwrap()
}

/// `turns` (a bone and how it's turned from rest) as one per bone.
fn turns_of(arm: &Armature, turns: &[(usize, Quat)]) -> Vec<Quat> {
    let mut all = vec![Quat::IDENTITY; arm.bones.len()];
    for &(b, q) in turns {
        all[b] = q;
    }
    all
}

/// `mesh`'s vertices posed with each bone turned by its turn.
fn posed(arm: &Armature, mesh: &EditMesh, skin: &Skin, turns: &[(usize, Quat)]) -> Vec<Vec3> {
    let matrices = arm.skin_matrices(&arm.pose(&turns_of(arm, turns)));
    let skinning = vertex_skinning(skin, mesh.vertex_count(), &matrices);
    mesh.positions()
        .iter()
        .zip(skinning)
        .map(|(&p, s)| s.map_or(p, |s| s.position(p)))
        .collect()
}

/// How cleanly `posed` bends `mesh`: the share of its triangles turned
/// over, and the most any edge stretched.
fn bend_quality(mesh: &EditMesh, posed: &[Vec3]) -> (f32, f32) {
    let rest = mesh.positions();
    let (mut flipped, mut total, mut stretch) = (0, 0, 0.0f32);
    for face in 0..mesh.face_count() as u32 {
        for triangle in mesh.triangles(face) {
            let v = triangle.map(|e| mesh.half_edge(e).vertex as usize);
            let before = (rest[v[1]] - rest[v[0]]).cross(rest[v[2]] - rest[v[0]]);
            let after = (posed[v[1]] - posed[v[0]]).cross(posed[v[2]] - posed[v[0]]);
            total += 1;
            if before.dot(after) < 0.0 {
                flipped += 1;
            }
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let length = rest[v[a]].distance(rest[v[b]]).max(1e-6);
                stretch = stretch.max(posed[v[a]].distance(posed[v[b]]) / length);
            }
        }
    }
    (flipped as f32 / total as f32, stretch)
}

/// The Walking character's skeleton from `hello-character`: Mixamo's own.
fn mixamo_rig() -> axcenity_assets::Rig {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-character/assets/models/Walking");
    let bytes = std::fs::read(folder.join("Walking.fbx")).unwrap();
    let model = load_model(&bytes, &folder, ModelFormat::Fbx).unwrap();
    model.rig.as_deref().unwrap().clone()
}

#[test]
fn the_humanoid_template_rests_with_mixamos_own_bone_names_offsets_and_turns() {
    let (arm, _) = humanoid();
    let (rig, place) = arm.to_rig();
    assert_eq!(rig.bones[0].name, ROOT_NAME);
    assert_eq!(rig.joints.len(), arm.bones.len());
    let mixamo = mixamo_rig();
    for (index, bone) in arm.bones.iter().enumerate() {
        let saved = &rig.bones[place[index]];
        let theirs = &mixamo.bones[mixamo.bone(&bone.name).expect("a Mixamo bone")];
        let parent = |rig: &axcenity_assets::Rig, b: &axcenity_assets::Bone| {
            b.parent.map(|p| rig.bones[p].name.clone())
        };
        // Mixamo's top bone is unnamed; the template's is the Armature.
        if bone.parent.is_some() {
            assert_eq!(
                parent(&rig, saved),
                parent(&mixamo, theirs),
                "{}",
                bone.name
            );
        }
        // A take keys these, so they have to be Mixamo's (centimetres).
        let gap = saved.rest.translation.distance(theirs.rest.translation);
        assert!(gap < 0.01, "{}: {gap} cm off", bone.name);
        let turn = saved.rest.rotation.angle_between(theirs.rest.rotation);
        assert!(turn < 1e-3, "{}: {turn} rad off", bone.name);
        assert!(saved.rest.scale.abs_diff_eq(Vec3::ONE, 1e-4));
    }
    // Fitted to the box: the head's tip at its top, the ground at its foot.
    let top = arm.bones[bone(&arm, "HeadTop_End")].head;
    assert!((top.y - HEIGHT).abs() < 1e-3, "{top}");
    assert!(arm.origin.y.abs() < 1e-6);
}

#[test]
fn automatic_weights_bend_a_humanoid_cleanly_at_its_elbows_and_knees() {
    let (arm, body) = humanoid();
    let skin = automatic_weights(&body, &arm);
    for vertex in 0..body.vertex_count() {
        let list = &skin.weights[vertex];
        assert!(
            !list.is_empty() && list.len() <= 4,
            "vertex {vertex}: {list:?}"
        );
        let total: f32 = list.iter().map(|(_, w)| w).sum();
        assert!((total - 1.0).abs() < 1e-4, "vertex {vertex}: {total}");
    }
    // A tip that doesn't deform has no weights.
    let tip = bone(&arm, "HeadTop_End");
    assert!(
        skin.weights
            .iter()
            .flatten()
            .all(|&(b, _)| b as usize != tip)
    );

    for (joint, child, above) in [
        ("LeftForeArm", "LeftHand", "LeftArm"),
        ("LeftLeg", "LeftFoot", "LeftUpLeg"),
    ] {
        let b = bone(&arm, joint);
        let turn = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let moved = posed(&arm, &body, &skin, &[(b, turn)]);
        let (flipped, stretch) = bend_quality(&body, &moved);
        assert!(
            flipped < 0.01,
            "{joint}: {flipped} of the triangles turned over"
        );
        assert!(stretch < 1.8, "{joint}: an edge stretched {stretch} times");
        // The bone's own part (past its middle) goes with it, whole.
        let matrix = arm.skin_matrices(&arm.pose(&turns_of(&arm, &[(b, turn)])))[b];
        let joint_bone = &arm.bones[b];
        let (head, tail) = (joint_bone.head, arm.bones[bone(&arm, child)].tail());
        let along = |p: Vec3| (p - head).dot(tail - head) / (tail - head).length_squared();
        let near = |p: Vec3, a: Vec3, b: Vec3| {
            let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            p.distance(a + (b - a) * t)
        };
        let upper = &arm.bones[bone(&arm, above)];
        // The same limb on the other side.
        let other = |name: &str| bone(&arm, &mirror_name(name).unwrap());
        let (other_head, other_tail) =
            (arm.bones[other(joint)].head, arm.bones[other(child)].tail());
        let mut checked = (0, 0);
        for (k, &p) in body.positions().iter().enumerate() {
            let on_limb = near(p, head, tail) < near(p, upper.head, head)
                && near(p, head, tail) < near(p, other_head, other_tail);
            if on_limb && along(p) > 0.5 && along(p) < 1.0 && near(p, head, tail) < 0.08 * HEIGHT {
                let gap = moved[k].distance(matrix.transform_point3(p));
                assert!(gap < 0.01 * HEIGHT, "{joint}: vertex {k} at {p} lags {gap}");
                checked.0 += 1;
            }
            // Above the joint's parent bone, nothing moves.
            if along(p) < -1.2 || near(p, head, tail) > 0.3 * HEIGHT {
                let gap = moved[k].distance(p);
                assert!(
                    gap < 0.005 * HEIGHT,
                    "{joint}: vertex {k} far away moved {gap}"
                );
                checked.1 += 1;
            }
        }
        assert!(checked.0 > 50 && checked.1 > 1000, "{joint}: {checked:?}");
    }
}

#[test]
fn weights_by_distance_give_each_vertex_its_nearest_bones() {
    let (arm, body) = humanoid();
    let skin = weights_by_distance(&body, &arm);
    let hand = bone(&arm, "LeftHand");
    let tip = arm.bones[hand].tail();
    let nearest = (0..body.vertex_count())
        .min_by(|&a, &b| {
            let (pa, pb) = (body.position(a as u32), body.position(b as u32));
            pa.distance(tip).total_cmp(&pb.distance(tip))
        })
        .unwrap();
    let (bones, weights) = skin.influences(nearest);
    assert_eq!(bones[0] as usize, hand, "{bones:?} {weights:?}");
    for vertex in 0..body.vertex_count() {
        assert!(skin.weights[vertex].len() <= 4);
        let total: f32 = skin.weights[vertex].iter().map(|(_, w)| w).sum();
        assert!((total - 1.0).abs() < 1e-4);
    }
}

/// A strip of quads along X from 0 to 4, two bones along it (`A` then
/// `B`), weighted half to each.
fn strip() -> (Armature, EditMesh, Skin) {
    let mut arm = Armature::default();
    let a = arm.add_bone("A", Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), None);
    arm.add_bone(
        "B",
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(4.0, 0.0, 0.0),
        Some(a),
    );
    let mut positions = Vec::new();
    let mut polygons = Vec::new();
    for k in 0..=8 {
        positions.push(Vec3::new(k as f32 * 0.5, 0.0, 0.0));
        positions.push(Vec3::new(k as f32 * 0.5, 0.0, 1.0));
    }
    for k in 0..8u32 {
        polygons.push(crate::half_edge::Polygon {
            corners: vec![2 * k, 2 * k + 1, 2 * k + 3, 2 * k + 2],
            uvs: vec![glam::Vec2::ZERO; 4],
            uvs2: Vec::new(),
            normals: Vec::new(),
            material: 0,
            smooth: false,
        });
    }
    let mesh = EditMesh::new(positions, &polygons);
    let weights = vec![vec![(0, 0.5), (1, 0.5)]; mesh.vertex_count()];
    let skin = Skin::new(&mesh, weights);
    (arm, mesh, skin)
}

#[test]
fn weight_brushes_add_subtract_smooth_and_blur_and_keep_each_vertex_adding_up_to_one() {
    let (_, mesh, start) = strip();
    let painter = WeightPainter::new(&mesh);
    let brush = |tool| WeightBrush {
        tool,
        strength: 1.0,
        falloff: BrushFalloff::Linear,
        ..WeightBrush::default()
    };
    let total = |skin: &Skin, v: usize| skin.weight(v, 0) + skin.weight(v, 1);

    // Add round x = 1: all of A in the middle, less further out, none
    // past the radius; B gives way, so each still adds up to 1.
    let mut skin = start.clone();
    assert!(painter.dab(
        &mut skin,
        &brush(WeightTool::Add),
        0,
        Vec3::new(1.0, 0.0, 0.0),
        1.2
    ));
    assert!((skin.weight(2 * 2, 0) - 1.0).abs() < 1e-5, "x = 1 is A's");
    let halfway = skin.weight(2 * 3, 0);
    assert!(halfway > 0.5 && halfway < 1.0, "{halfway}");
    assert!(
        (skin.weight(2 * 6, 0) - 0.5).abs() < 1e-6,
        "x = 3 is untouched"
    );
    for v in 0..mesh.vertex_count() {
        assert!((total(&skin, v) - 1.0).abs() < 1e-5);
    }

    // Subtract takes A off where it's dabbed.
    let mut taken = start.clone();
    painter.dab(
        &mut taken,
        &brush(WeightTool::Subtract),
        0,
        Vec3::new(1.0, 0.0, 0.0),
        1.2,
    );
    assert!(taken.weight(2 * 2, 0) < 1e-4 && (taken.weight(2 * 2, 1) - 1.0).abs() < 1e-5);

    // Smooth evens out a step in A's weights; Blur evens every bone's.
    let mut stepped = start.clone();
    for v in 0..mesh.vertex_count() {
        let x = mesh.position(v as u32).x;
        let a = if x < 2.0 { 0.95 } else { 0.05 };
        stepped.weights[v] = vec![(0, a), (1, 1.0 - a)];
    }
    let jump = |skin: &Skin| skin.weight(2 * 3, 0) - skin.weight(2 * 4, 0);
    for tool in [WeightTool::Smooth, WeightTool::Blur] {
        let mut smoothed = stepped.clone();
        painter.dab(
            &mut smoothed,
            &brush(tool),
            0,
            Vec3::new(2.0, 0.0, 0.5),
            1.5,
        );
        assert!(
            jump(&smoothed) < jump(&stepped) * 0.8,
            "{tool:?}: {}",
            jump(&smoothed)
        );
        for v in 0..mesh.vertex_count() {
            assert!((total(&smoothed, v) - 1.0).abs() < 1e-4, "{tool:?}");
        }
    }
}

#[test]
fn normalize_limit_and_influences_give_four_bones_adding_up_to_one() {
    let (_, mesh, mut skin) = strip();
    skin.weights[0] = vec![(0, 0.9), (1, 0.6), (2, 0.3), (3, 0.2), (4, 0.1)];
    let (bones, weights) = skin.influences(0);
    assert_eq!(bones, [0, 1, 2, 3]);
    let sum: f32 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6);
    // The fifth's weight comes off the four kept: 0.8, 0.5, 0.2, 0.1.
    assert!((weights[0] - 0.8 / 1.6).abs() < 1e-6, "{weights:?}");
    // So two vertices side by side whose fourth and fifth swap move
    // almost alike, though each keeps a bone the other drops.
    skin.weights[1] = vec![(0, 0.9), (1, 0.6), (2, 0.3), (3, 0.15), (4, 0.1501)];
    skin.weights[2] = vec![(0, 0.9), (1, 0.6), (2, 0.3), (3, 0.1501), (4, 0.15)];
    let (a, b) = (skin.influences(1), skin.influences(2));
    assert_ne!(a.0, b.0);
    let weight_of = |(bones, weights): ([u16; 4], [f32; 4]), bone: u16| {
        bones
            .iter()
            .zip(weights)
            .filter(|(b, _)| **b == bone)
            .map(|(_, w)| w)
            .sum::<f32>()
    };
    for bone in 0..5 {
        assert!(
            (weight_of(a, bone) - weight_of(b, bone)).abs() < 1e-3,
            "bone {bone}"
        );
    }
    skin.limit(4);
    assert_eq!(skin.weights[0].len(), 4);
    skin.normalize();
    let sum: f32 = skin.weights[0].iter().map(|(_, w)| w).sum();
    assert!((sum - 1.0).abs() < 1e-6);
    // A mesh with other vertices takes each one's nearest weights.
    let finer = crate::sculpt::subdivide(&mesh).unwrap();
    assert!(finer.vertex_count() > mesh.vertex_count());
    let fitted = skin.fitted(&finer);
    assert_eq!(fitted.weights.len(), finer.vertex_count());
    let corner = (0..finer.vertex_count())
        .min_by(|&a, &b| {
            let (pa, pb) = (finer.position(a as u32), finer.position(b as u32));
            pa.length().total_cmp(&pb.length())
        })
        .unwrap();
    assert_eq!(fitted.weights[corner], skin.weights[0]);
    assert_eq!(skin.fitted(&mesh).weights, skin.weights);
}

#[test]
fn mirror_weights_copy_one_side_onto_the_other_with_left_and_right_swapped() {
    let sphere = crate::PrimitiveSettings::of(crate::PrimitiveShape::UvSphere).build();
    let mut arm = Armature::default();
    let middle = arm.add_bone(
        "Middle",
        Vec3::new(0.0, -0.5, 0.0),
        Vec3::new(0.0, 0.5, 0.0),
        None,
    );
    let left = arm.add_bone(
        "Side.L",
        Vec3::new(0.1, 0.0, 0.0),
        Vec3::new(0.5, 0.0, 0.0),
        Some(middle),
    );
    arm.symmetrize(&[left]);
    let right = arm.bone("Side.R").unwrap();
    let mut skin = Skin::new(
        &sphere,
        vec![vec![(middle as u16, 1.0)]; sphere.vertex_count()],
    );
    let painter = WeightPainter::new(&sphere);
    let brush = WeightBrush {
        strength: 0.8,
        falloff: BrushFalloff::Linear,
        ..WeightBrush::default()
    };
    let at = Vec3::new(0.9, 0.2, 0.1);
    painter.dab(&mut skin, &brush, left, at, 0.6);
    let changed = mirror_weights(&mut skin, &sphere, &arm, true);
    assert!(changed > 0);
    let mut compared = 0;
    for (k, &p) in sphere.positions().iter().enumerate() {
        if p.x < -1e-3 {
            let image = (0..sphere.vertex_count())
                .find(|&m| {
                    sphere
                        .position(m as u32)
                        .distance(p * Vec3::new(-1.0, 1.0, 1.0))
                        < 1e-4
                })
                .unwrap();
            assert!(
                (skin.weight(k, right) - skin.weight(image, left)).abs() < 1e-6,
                "vertex {k}"
            );
            assert_eq!(skin.weight(k, left), 0.0);
            assert!((skin.weight(k, middle) - skin.weight(image, middle)).abs() < 1e-6);
            compared += (skin.weight(k, right) > 0.0) as usize;
        }
    }
    assert!(compared > 5, "{compared}");
}

#[test]
fn armature_edits_extrude_parent_rename_roll_delete_and_symmetrize() {
    assert_eq!(mirror_name("upper_arm.L").as_deref(), Some("upper_arm.R"));
    assert_eq!(mirror_name("hand.R.001").as_deref(), Some("hand.L.001"));
    assert_eq!(mirror_name("Foot_l").as_deref(), Some("Foot_r"));
    assert_eq!(
        mirror_name("mixamorig:LeftArm").as_deref(),
        Some("mixamorig:RightArm")
    );
    assert_eq!(mirror_name("Spine"), None);

    let mut arm = Armature::default();
    let root = arm.add_bone("Root", Vec3::ZERO, Vec3::Y, None);
    let upper = arm.add_bone(
        "upper_arm.L",
        Vec3::new(0.2, 1.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Some(root),
    );
    // E: a child from the tail, the same way, as long, named after it.
    let lower = arm.extrude(upper).unwrap();
    assert_eq!(arm.bones[lower].name, "upper_arm.L.001");
    assert_eq!(arm.bones[lower].parent, Some(upper));
    assert!(
        arm.bones[lower]
            .head
            .abs_diff_eq(Vec3::new(1.0, 1.0, 0.0), 1e-5)
    );
    assert!(
        arm.bones[lower]
            .tail()
            .abs_diff_eq(Vec3::new(1.8, 1.0, 0.0), 1e-5)
    );
    assert_eq!(arm.rename(lower, "forearm.L"), "forearm.L");
    assert_eq!(
        arm.rename(lower, "upper_arm.L"),
        "upper_arm.L.001",
        "names stay unique"
    );
    arm.rename(lower, "forearm.L");
    // No bone goes inside itself.
    assert!(!arm.set_parent(root, Some(lower)));
    assert!(arm.set_parent(lower, Some(root)));
    assert!(arm.set_parent(lower, Some(upper)));
    // Roll turns it round its own length, which stays.
    let tail = arm.bones[upper].tail();
    arm.bones[upper].set_roll(0.7);
    assert!((arm.bones[upper].roll() - 0.7).abs() < 1e-4);
    assert!(arm.bones[upper].tail().abs_diff_eq(tail, 1e-5));
    // Symmetrize makes the right side, mirrored, the copies' parents the
    // mirrors of the originals' where there are any.
    let made = arm.symmetrize(&[upper, lower]);
    assert_eq!(made.len(), 2);
    let right = arm.bone("upper_arm.R").unwrap();
    let right_lower = arm.bone("forearm.R").unwrap();
    assert_eq!(arm.bones[right_lower].parent, Some(right));
    assert_eq!(arm.bones[right].parent, Some(root));
    assert!(
        arm.bones[right]
            .head
            .abs_diff_eq(Vec3::new(-0.2, 1.0, 0.0), 1e-5)
    );
    assert!(
        arm.bones[right]
            .tail()
            .abs_diff_eq(Vec3::new(-1.0, 1.0, 0.0), 1e-5)
    );
    assert!(
        (arm.bones[right].roll() + 0.7).abs() < 1e-3,
        "{}",
        arm.bones[right].roll()
    );
    // Deleting a bone hands its children to its parent and its weights go.
    let mut skin = Skin {
        weights: vec![vec![(upper as u16, 0.5), (lower as u16, 0.5)]],
        ..Skin::default()
    };
    let map = arm.delete(upper);
    skin.remap(&map);
    let lower_now = arm.bone("forearm.L").unwrap();
    assert_eq!(arm.bones[lower_now].parent, Some(root));
    assert_eq!(skin.weights[0], vec![(lower_now as u16, 0.5)]);
}

#[test]
fn a_rig_read_back_from_its_bones_is_the_same_armature() {
    let (arm, _) = humanoid();
    let (rig, _) = arm.to_rig();
    let (back, map) = Armature::from_rig(&rig);
    assert_eq!(map[0], None, "the Armature bone is the root");
    assert!((back.unit - arm.unit).abs() < 1e-6 && back.origin.abs_diff_eq(arm.origin, 1e-5));
    for bone in &arm.bones {
        let other = &back.bones[back.bone(&bone.name).unwrap()];
        assert!(other.head.abs_diff_eq(bone.head, 1e-4), "{}", bone.name);
        assert!(
            other.rotation.angle_between(bone.rotation) < 1e-3,
            "{}",
            bone.name
        );
    }
    let one = ArmBone::new("Bone", Vec3::ZERO, Vec3::new(0.0, 0.0, 2.0), None);
    assert!(one.tail().abs_diff_eq(Vec3::new(0.0, 0.0, 2.0), 1e-6));
}

#[test]
fn a_skinned_humanoid_saves_as_a_glb_the_game_poses_as_the_studio_does_and_reopens_rigged() {
    let (arm, body) = humanoid();
    let skin = automatic_weights(&body, &arm);
    let mut doc = ModelDoc::primitive(&crate::PrimitiveSettings::default());
    doc.objects = vec![DocObject {
        name: "Body".to_owned(),
        mesh: Some(body.clone()),
        skin: Some(skin.clone()),
        ..doc.objects[0].clone()
    }];
    doc.armature = Some(arm.clone());
    let glb = doc.write_glb();

    // The game's reading: a rig named as the armature, posed on the CPU
    // just as the Studio's Pose test skins it.
    let model = load_model(&glb, Path::new("."), ModelFormat::Gltf).unwrap();
    let rig = model.rig.as_deref().expect("a skinned .glb has a rig");
    assert_eq!(rig.bones[0].name, ROOT_NAME);
    assert_eq!(rig.bones.len(), arm.bones.len() + 1);
    let elbow = bone(&arm, "LeftForeArm");
    let knee = bone(&arm, "RightLeg");
    let turns = [
        (elbow, Quat::from_rotation_x(-1.2)),
        (knee, Quat::from_rotation_x(0.9)),
    ];
    let pose = arm.pose(&turns_of(&arm, &turns));
    // The same bones in the file's order, found by name as a take would.
    let (ours, _) = arm.to_rig();
    let game_pose: Vec<_> = rig
        .bones
        .iter()
        .map(|b| pose[ours.bone(&b.name).unwrap()])
        .collect();
    let posed_model = model.pose(&game_pose).unwrap();
    let studio = posed(&arm, &body, &skin, &turns);
    let unfit = rig.fit.inverse();
    let mut compared = 0;
    for (index, primitive) in model.primitives.iter().enumerate() {
        let game = posed_model.primitive(&model, index, 0);
        for (corner, &rest) in primitive.positions.iter().enumerate() {
            let rest = unfit.transform_point3(Vec3::from(rest));
            let vertex = (0..body.vertex_count())
                .find(|&v| body.position(v as u32).distance(rest) < 1e-4)
                .unwrap();
            let there = unfit.transform_point3(Vec3::from(game.positions[corner]));
            let gap = there.distance(studio[vertex]);
            assert!(gap < 1e-3, "corner {corner}: {gap}");
            compared += 1;
            if compared > 3000 {
                break;
            }
        }
    }
    assert!(compared > 1000);

    // Opened again: the armature from the bones, the weights on the mesh.
    let reopened = ModelDoc::open(&glb, Path::new("."), ModelFormat::Gltf).unwrap();
    assert_eq!(reopened.objects.len(), 1, "the bones aren't objects");
    let back = reopened.armature.as_ref().unwrap();
    assert_eq!(back.bones.len(), arm.bones.len());
    let object = &reopened.objects[0];
    let mesh = object.mesh.as_ref().unwrap();
    let back_skin = object.skin.as_ref().unwrap();
    for v in (0..mesh.vertex_count()).step_by(7) {
        let original = (0..body.vertex_count())
            .find(|&k| body.position(k as u32).distance(mesh.position(v as u32)) < 1e-4)
            .unwrap();
        let (bones, weights) = skin.influences(original);
        for (b, w) in bones.iter().zip(weights).filter(|(_, w)| *w > 0.0) {
            let name = &arm.bones[*b as usize].name;
            let there = back_skin.weight(v, back.bone(name).unwrap());
            assert!(
                (there - w).abs() < 1e-5,
                "vertex {v} {name}: {there} vs {w}"
            );
        }
    }

    // With the sidecar, the armature comes back exactly.
    let sidecar = doc
        .write_sidecar(&glb)
        .expect("an armature needs a sidecar");
    let mut exact = reopened.clone();
    exact.read_sidecar(&sidecar, &glb).unwrap();
    assert_eq!(exact.armature.as_ref(), Some(&arm));
}
