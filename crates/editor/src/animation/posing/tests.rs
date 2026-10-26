use std::path::Path;
use std::sync::Arc;

use axcenity_assets::{Assets, Model};
use axcenity_core::{ClassName, Value, Vector3};
use axcenity_render::Camera;

use super::*;

fn noodle() -> Arc<Model> {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation");
    Assets::new(example)
        .model("assets/models/noodle.fbx")
        .unwrap()
}

/// A MeshPart twice the noodle's size, turned and moved.
fn part(world: &mut World, model: &Model) -> InstanceId {
    let part = world.create(ClassName::MeshPart).unwrap();
    let [x, y, z] = model.size.map(|length| length * 2.0);
    for (name, value) in [
        ("Size", Value::Vector3(Vector3::new(x, y, z))),
        ("Position", Value::Vector3(Vector3::new(4.0, 1.5, 0.0))),
        ("Orientation", Value::Vector3(Vector3::new(0.0, 90.0, 0.0))),
    ] {
        world.set_property(part, name, value).unwrap();
    }
    part
}

#[test]
fn turning_a_bone_in_the_world_turns_its_joint_that_way() {
    let model = noodle();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let pose = rig.rest_pose();
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let (mid, top) = (rig.bone("Mid").unwrap(), rig.bone("Top").unwrap());
    let joints = skeleton.joints();
    let turn = Quat::from_rotation_x(0.5);
    let turned = skeleton.turned(mid, turn).unwrap();
    let mut after = pose.clone();
    after[mid] = turned;
    let moved = Skeleton::on(&world, part, rig, &after, &[])
        .unwrap()
        .joints();
    // Mid stays put; Top swings around Mid by the same turn in the world.
    assert!(moved[mid].abs_diff_eq(joints[mid], 1e-4));
    let expected = joints[mid] + turn * (joints[top] - joints[mid]);
    assert!(
        moved[top].abs_diff_eq(expected, 1e-3),
        "{:?} vs {expected:?}",
        moved[top]
    );
}

#[test]
fn moving_the_root_moves_its_joint_by_as_much_in_the_world() {
    let model = noodle();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let pose = rig.rest_pose();
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let root = rig.bone("Root").unwrap();
    assert!(skeleton.is_root(root));
    assert!(!skeleton.is_root(rig.bone("Mid").unwrap()));
    let by = Vec3::new(0.0, 0.5, 0.25);
    let mut after = pose.clone();
    after[root] = skeleton.moved(root, by).unwrap();
    let moved = Skeleton::on(&world, part, rig, &after, &[])
        .unwrap()
        .joints();
    assert!(moved[root].abs_diff_eq(skeleton.joints()[root] + by, 1e-3));
}

/// A camera `back` model heights in front of `joint` (and `off` to the
/// side and up).
fn camera_at(model: &Model, joint: Vec3, off: Vec3) -> Camera {
    let height = model.size.iter().copied().fold(0.0, f32::max) * 2.0;
    Camera {
        position: joint + (Vec3::Z * 3.0 + off) * height,
        look_at: joint,
        fov_y_degrees: 50.0,
    }
}

/// One frame of a preview filling a 640×480 window, seen through
/// `camera`: what posing found, and the screen it drew through.
#[allow(clippy::too_many_arguments)] // Each is a different input to the frame.
fn frame(
    ctx: &egui::Context,
    camera: Camera,
    poser: &mut Poser,
    skeleton: &Skeleton,
    settings: &PoseSettings,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> (Interaction, Screen) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(640.0, 480.0),
        )),
        events,
        modifiers,
        ..Default::default()
    };
    let mut out = None;
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let size = ui.available_size();
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                let screen = Screen::new(camera, rect);
                let done = poser.interact(ui, &response, &screen, skeleton, settings);
                out = Some((done, screen));
            });
    });
    out.expect("drawn")
}

/// Grabs the selected bone's `handle` and moves the mouse each of
/// `moves` points from where it grabbed, forward along the handle as
/// drawn there, with those keys held, then lets go.
/// Returns the bone's transform after each move.
fn drag_handle(
    camera: Camera,
    poser: &mut Poser,
    skeleton: &Skeleton,
    settings: &PoseSettings,
    handle: Handle,
    moves: &[(f32, egui::Modifiers)],
) -> Vec<BoneTransform> {
    let ctx = theme::test_context();
    let none = egui::Modifiers::NONE;
    let away = vec![egui::Event::PointerMoved(egui::pos2(2.0, 2.0))];
    let (done, _) = frame(&ctx, camera, poser, skeleton, settings, away, none);
    let shape = done
        .shapes
        .iter()
        .find(|shape| shape.handle == handle)
        .expect("the handle shows");
    let lines = shape.lines();
    let [a, b] = lines[lines.len() / 2];
    let grab = a.lerp(b, 0.5);
    let forward = (b - a).normalized();
    let at = |pos| vec![egui::Event::PointerMoved(pos)];
    frame(&ctx, camera, poser, skeleton, settings, at(grab), none);
    let press = |pressed| egui::Event::PointerButton {
        pos: grab,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: none,
    };
    frame(
        &ctx,
        camera,
        poser,
        skeleton,
        settings,
        vec![press(true)],
        none,
    );
    assert!(poser.drag.is_some(), "{handle:?} was grabbed");
    let mut posed = Vec::new();
    for &(by, modifiers) in moves {
        let (done, _) = frame(
            &ctx,
            camera,
            poser,
            skeleton,
            settings,
            at(grab + forward * by),
            modifiers,
        );
        let selected = poser.selected.expect("a bone is selected");
        match done.event {
            Some(BoneEvent::Pose(bones)) => posed.push(
                bones
                    .into_iter()
                    .find(|(bone, _)| *bone == selected)
                    .expect("the dragged bone is posed")
                    .1,
            ),
            other => panic!("the bone wasn't posed: {other:?}"),
        }
    }
    frame(
        &ctx,
        camera,
        poser,
        skeleton,
        settings,
        vec![press(false)],
        none,
    );
    posed
}

const CTRL: egui::Modifiers = egui::Modifiers::COMMAND;
const NONE: egui::Modifiers = egui::Modifiers::NONE;

/// How far `bone`'s joint is moved in the world with `transform`.
fn joint_moved(skeleton: &Skeleton, bone: usize, transform: BoneTransform) -> Vec3 {
    let mut pose = skeleton.pose.to_vec();
    pose[bone] = transform;
    let moved = Skeleton {
        pose: &pose,
        ..*skeleton
    }
    .joints();
    moved[bone] - skeleton.joints()[bone]
}

/// How many `step`s `distance` is, and how far that is from whole.
fn steps(distance: f32, step: f32) -> (f32, f32) {
    let steps = distance / step;
    (steps, (steps - steps.round()).abs())
}

#[test]
fn move_snap_moves_the_root_in_steps_and_ctrl_lets_it_go_free_while_held() {
    let model = noodle();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let pose = rig.rest_pose();
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let root = rig.bone("Root").unwrap();
    let camera = camera_at(&model, skeleton.joints()[root], Vec3::new(0.3, 0.4, 0.0));
    let mut poser = Poser {
        selected: Some(root),
        tool: Tool::Move,
        ..Poser::default()
    };
    let settings = PoseSettings::default();
    assert_eq!(settings.snap.move_step, 0.05, "it starts at 0.05 studs");
    let posed = drag_handle(
        camera,
        &mut poser,
        &skeleton,
        &settings,
        Handle::Move(gizmo::Axis::X),
        &[(23.0, NONE), (61.0, NONE), (61.0, CTRL), (61.0, NONE)],
    );
    let moved: Vec<Vec3> = posed
        .into_iter()
        .map(|transform| joint_moved(&skeleton, root, transform))
        .collect();
    for (i, by) in moved.iter().enumerate() {
        assert!(by.y.abs() < 1e-4 && by.z.abs() < 1e-4, "only along X: {by}");
        assert!(by.x > 0.0, "move {i} goes right: {by}");
    }
    let (count, off) = steps(moved[0].x, 0.05);
    assert!(
        off < 1e-2 && count >= 1.0,
        "a whole number of steps: {count}"
    );
    assert!(steps(moved[1].x, 0.05).1 < 1e-2);
    // Ctrl: free, off the steps; letting go snaps back to the step.
    let (_, free_off) = steps(moved[2].x, 0.05);
    assert!(free_off > 0.02, "free with Ctrl: {}", moved[2].x);
    assert!((moved[3].x - moved[1].x).abs() < 1e-4, "snapped again");
    assert!((moved[3].x - (moved[2].x / 0.05).round() * 0.05).abs() < 1e-4);
}

#[test]
fn ctrl_flips_snapping_for_turns_and_moves_whether_it_is_off_or_on() {
    let model = noodle();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let pose = rig.rest_pose();
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let top = rig.bone("Top").unwrap();
    let root = rig.bone("Root").unwrap();
    let joints = skeleton.joints();
    let camera = camera_at(&model, joints[top], Vec3::ZERO);
    // How far each drag turns Top with its Z ring (facing the camera),
    // in degrees, with and without Ctrl.
    let turns = |settings: &PoseSettings| -> Vec<f32> {
        let mut poser = Poser {
            selected: Some(top),
            ..Poser::default()
        };
        let along = 50.0;
        drag_handle(
            camera,
            &mut poser,
            &skeleton,
            settings,
            Handle::Rotate(gizmo::Axis::Z),
            &[(along, NONE), (along, CTRL), (along, NONE)],
        )
        .into_iter()
        .map(|t| t.rotation.angle_between(pose[top].rotation).to_degrees())
        .collect()
    };
    let mut fifteen = PoseSettings::default();
    fifteen.snap.rotate_step = 15.0;
    let snapped = turns(&fifteen);
    assert!(
        steps(snapped[0], 15.0).1 < 1e-2 && snapped[0] > 1.0,
        "{snapped:?}"
    );
    assert!(
        steps(snapped[1], 15.0).1 > 0.05,
        "Ctrl turns freely: {snapped:?}"
    );
    assert_eq!(snapped[0], snapped[2], "letting go snaps again");

    let mut off = fifteen;
    off.snap.rotate_on = false;
    let free = turns(&off);
    assert!(steps(free[0], 15.0).1 > 0.05, "Off turns freely: {free:?}");
    assert!((free[0] - snapped[1]).abs() < 1e-2, "as freely as Ctrl did");
    assert!(
        (free[1] - snapped[0]).abs() < 1e-2,
        "Ctrl snaps to the step Off kept"
    );
    assert!((free[2] - free[0]).abs() < 1e-3);

    // Moves too: Off with Ctrl moves in the kept 0.05-stud steps.
    let mut poser = Poser {
        selected: Some(root),
        tool: Tool::Move,
        ..Poser::default()
    };
    let mut no_move_snap = PoseSettings::default();
    no_move_snap.snap.move_on = false;
    let camera = camera_at(&model, joints[root], Vec3::new(0.3, 0.4, 0.0));
    let right = 61.0;
    let moved: Vec<f32> = drag_handle(
        camera,
        &mut poser,
        &skeleton,
        &no_move_snap,
        Handle::Move(gizmo::Axis::X),
        &[(right, NONE), (right, CTRL)],
    )
    .into_iter()
    .map(|t| joint_moved(&skeleton, root, t).x)
    .collect();
    assert!(steps(moved[0], 0.05).1 > 0.02, "free when Off: {moved:?}");
    assert!(
        steps(moved[1], 0.05).1 < 1e-2,
        "stepped with Ctrl: {moved:?}"
    );
}

#[test]
fn world_and_local_turn_the_gizmo_without_moving_the_bone() {
    let model = noodle();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let pose = rig.rest_pose();
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let top = rig.bone("Top").unwrap();
    let camera = camera_at(&model, skeleton.joints()[top], Vec3::new(0.8, 0.6, 0.0));
    let mut poser = Poser {
        selected: Some(top),
        ..Poser::default()
    };
    let mut settings = PoseSettings::default();
    settings.snap.rotate_on = false;
    let bone_turn = skeleton.gizmo_pose(top).unwrap().rotation;
    assert!(
        bone_turn.angle_between(Quat::IDENTITY) > 0.1,
        "Top is turned in the world"
    );

    // Switching moves only the gizmo: same place, the bone's own turn,
    // and nothing is posed.
    let (_, _, _, world_frame) = poser.gizmo_frame(&skeleton, &settings).unwrap();
    settings.axes = Axes::Local;
    let (_, _, _, local_frame) = poser.gizmo_frame(&skeleton, &settings).unwrap();
    assert!(world_frame.position.abs_diff_eq(local_frame.position, 1e-5));
    assert!(world_frame.rotation.angle_between(Quat::IDENTITY) < 1e-5);
    assert!(local_frame.rotation.angle_between(bone_turn) < 1e-5);
    let ctx = theme::test_context();
    for axes in [Axes::World, Axes::Local, Axes::World] {
        settings.axes = axes;
        let still = vec![egui::Event::PointerMoved(egui::pos2(2.0, 2.0))];
        let (done, _) = frame(&ctx, camera, &mut poser, &skeleton, &settings, still, NONE);
        assert_eq!(done.event, None, "switching to {axes:?} poses nothing");
    }

    // Dragging the X ring turns Top around the world's X, or its own.
    let turn_axis = |settings: &PoseSettings, poser: &mut Poser| {
        let posed = drag_handle(
            camera,
            poser,
            &skeleton,
            settings,
            Handle::Rotate(gizmo::Axis::X),
            &[(30.0, NONE)],
        );
        let mut after = pose.clone();
        after[top] = posed[0];
        let turned = Skeleton {
            pose: &after,
            ..skeleton
        }
        .gizmo_pose(top)
        .unwrap()
        .rotation;
        let (axis, angle) = (turned * bone_turn.inverse()).to_axis_angle();
        assert!(angle > 0.05, "it turned");
        // Turning around the joint leaves the joint where it was.
        let joint = Skeleton {
            pose: &after,
            ..skeleton
        }
        .joints()[top];
        assert!(joint.abs_diff_eq(skeleton.joints()[top], 1e-4));
        axis
    };
    settings.axes = Axes::World;
    let around = turn_axis(&settings, &mut poser);
    assert!(around.dot(Vec3::X).abs() > 0.999, "the world's X: {around}");
    settings.axes = Axes::Local;
    let around = turn_axis(&settings, &mut poser);
    let own_x = bone_turn * Vec3::X;
    assert!(
        around.dot(own_x).abs() > 0.999,
        "Top's own X: {around} vs {own_x}"
    );
}

#[test]
fn shift_turns_a_bone_a_quarter_as_fast_and_letting_go_does_not_jump() {
    let model = noodle();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let pose = rig.rest_pose();
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let top = rig.bone("Top").unwrap();
    let camera = camera_at(&model, skeleton.joints()[top], Vec3::ZERO);
    let mut poser = Poser {
        selected: Some(top),
        ..Poser::default()
    };
    let mut settings = PoseSettings::default();
    settings.snap.rotate_on = false;
    let shift = egui::Modifiers::SHIFT;
    let angles: Vec<f32> = drag_handle(
        camera,
        &mut poser,
        &skeleton,
        &settings,
        Handle::Rotate(gizmo::Axis::Z),
        &[(40.0, NONE), (80.0, shift), (80.0, NONE), (120.0, NONE)],
    )
    .into_iter()
    .map(|t| t.rotation.angle_between(pose[top].rotation))
    .collect();
    let normal = angles[0];
    assert!(
        (normal - 40.0 * gizmo::TURN_PER_POINT).abs() < 1e-3,
        "{angles:?}"
    );
    assert!(
        (angles[1] - normal * 1.25).abs() < 1e-3,
        "a quarter: {angles:?}"
    );
    assert!((angles[2] - angles[1]).abs() < 1e-4, "no jump: {angles:?}");
    assert!(
        (angles[3] - normal * 2.25).abs() < 1e-3,
        "full speed: {angles:?}"
    );
}

#[test]
fn the_tree_leaves_out_unnamed_bones() {
    let model = noodle();
    let rig = model.rig.as_ref().unwrap();
    let rows: Vec<(&str, usize)> = bone_rows(rig)
        .into_iter()
        .map(|(bone, depth)| (rig.bones[bone].name.as_str(), depth))
        .collect();
    assert_eq!(rows, [("Root", 0), ("Mid", 1), ("Top", 2), ("Hat", 3)]);
}

/// The names of the shown rows of `model`'s tree.
fn shown(folds: &mut Folds, model: &str, rig: &Rig, keys: &[usize]) -> Vec<String> {
    folds
        .rows(model, rig, keys)
        .into_iter()
        .map(|row| rig.bones[row.bone].name.clone())
        .collect()
}

#[test]
fn a_mixamo_rigs_bone_tree_folds_unfolds_and_collapses_to_its_root() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-events");
    let walking = "assets/models/Walking/Walking.fbx";
    let model = Assets::new(example).model(walking).unwrap();
    let rig = model.rig.as_deref().unwrap();
    let all = bone_rows(rig);
    assert!(all.len() > 40, "a Mixamo rig has many bones");
    let no_keys = vec![0; rig.bones.len()];
    let mut folds = Folds::default();

    // It opens with three levels showing: Hips, Spine, LeftUpLeg… but
    // not what's under them.
    let rows = folds.rows(walking, rig, &no_keys);
    assert!(rows.iter().all(|row| row.depth < OPEN_LEVELS));
    assert!(rows.len() < all.len());
    let spine = rig.bone("mixamorig:Spine").unwrap();
    let spine1 = rig.bone("mixamorig:Spine1").unwrap();
    let spine2 = rig.bone("mixamorig:Spine2").unwrap();
    let row_of = |rows: &[TreeRow], bone| rows.iter().find(|row| row.bone == bone).cloned();
    assert_eq!(row_of(&rows, spine).unwrap().folded, Some(false));
    assert_eq!(row_of(&rows, spine1).unwrap().folded, Some(true));
    assert!(row_of(&rows, spine2).is_none());

    // Unfolding Spine1 shows Spine2 (folded, as it's below the third
    // level); folding Spine hides both.
    folds.toggle(walking, rig, spine1);
    let rows = folds.rows(walking, rig, &no_keys);
    assert_eq!(row_of(&rows, spine2).unwrap().folded, Some(true));
    folds.toggle(walking, rig, spine);
    let rows = folds.rows(walking, rig, &no_keys);
    assert!(row_of(&rows, spine1).is_none() && row_of(&rows, spine2).is_none());
    // A folded bone shows a dot when a bone under it has keys.
    let mut keys = no_keys.clone();
    keys[rig.bone("mixamorig:Head").unwrap()] = 3;
    assert!(
        row_of(&folds.rows(walking, rig, &keys), spine)
            .unwrap()
            .hidden_keys
    );
    // Unfolding Spine again brings Spine1 back as it was: unfolded.
    folds.toggle(walking, rig, spine);
    let rows = folds.rows(walking, rig, &no_keys);
    assert_eq!(row_of(&rows, spine1).unwrap().folded, Some(false));

    // Collapse all leaves only the root; Expand all shows every bone.
    folds.collapse_all(walking, rig);
    assert_eq!(
        shown(&mut folds, walking, rig, &no_keys),
        ["mixamorig:Hips"]
    );
    folds.expand_all(walking, rig);
    assert_eq!(folds.rows(walking, rig, &no_keys).len(), all.len());

    // Each model keeps its own folds.
    let noodle = noodle();
    let noodle_rig = noodle.rig.as_deref().unwrap();
    folds.collapse_all("assets/models/noodle.fbx", noodle_rig);
    assert_eq!(folds.rows(walking, rig, &no_keys).len(), all.len());
    assert_eq!(
        shown(&mut folds, "assets/models/noodle.fbx", noodle_rig, &[0; 8]),
        ["Root"]
    );
}

fn mixamo() -> Arc<Model> {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-events");
    Assets::new(example)
        .model("assets/models/Walking/Walking.fbx")
        .unwrap()
}

/// How far each named bone's joint is from its named parent's, in the
/// rig's file space.
fn bone_lengths(rig: &Rig, pose: &[BoneTransform]) -> Vec<f32> {
    let joints = ik::joint_positions(rig, pose);
    named_parents(rig)
        .iter()
        .enumerate()
        .map(|(bone, parent)| parent.map_or(0.0, |p| joints[p].distance(joints[bone])))
        .collect()
}

fn assert_lengths_kept(rig: &Rig, before: &[BoneTransform], after: &[BoneTransform]) {
    let (before, after) = (bone_lengths(rig, before), bone_lengths(rig, after));
    for (bone, (a, b)) in before.iter().zip(&after).enumerate() {
        assert!(
            (a - b).abs() <= a * 1e-4 + 1e-5,
            "{}: {a} became {b}",
            rig.bones[bone].name
        );
    }
}

/// `pose` with `bones` posed.
fn with(pose: &[BoneTransform], bones: &[(usize, BoneTransform)]) -> Vec<BoneTransform> {
    let mut pose = pose.to_vec();
    for (bone, transform) in bones {
        pose[*bone] = *transform;
    }
    pose
}

#[test]
fn dragging_a_hand_moves_it_to_the_mouse_turning_the_arms_two_bones() {
    let model = mixamo();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let bone = |name: &str| rig.bone(&format!("mixamorig:{name}")).unwrap();
    let (hand, forearm, arm) = (bone("LeftHand"), bone("LeftForeArm"), bone("LeftArm"));
    // The rest pose's arm is straight, so the hand can only go further
    // off; bend the elbow first.
    let mut pose = rig.rest_pose();
    pose[forearm].rotation *= Quat::from_rotation_z(1.0);
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let joint = skeleton.joints()[hand];
    let camera = camera_at(&model, joint, Vec3::new(0.3, 0.2, 0.0));
    let mut poser = Poser {
        selected: Some(hand),
        tool: Tool::Move,
        ..Poser::default()
    };
    let mut settings = PoseSettings::default();
    settings.snap.move_on = false;
    assert_eq!(
        settings.chain, 2,
        "the parent and grandparent to start with"
    );

    // Grab the hand's joint itself and drag it up and to the side.
    let ctx = theme::test_context();
    let (_, screen) = frame(&ctx, camera, &mut poser, &skeleton, &settings, vec![], NONE);
    let grab = screen.point(joint).unwrap();
    let at = |pos| vec![egui::Event::PointerMoved(pos)];
    let press = |pressed| egui::Event::PointerButton {
        pos: grab,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: NONE,
    };
    frame(
        &ctx,
        camera,
        &mut poser,
        &skeleton,
        &settings,
        at(grab),
        NONE,
    );
    frame(
        &ctx,
        camera,
        &mut poser,
        &skeleton,
        &settings,
        vec![press(true)],
        NONE,
    );
    assert!(
        matches!(
            poser.drag,
            Some(Dragging {
                grip: Grip::Free { .. },
                ..
            })
        ),
        "the joint itself was grabbed"
    );
    // Toward the shoulder, and a little across, so the hand can get
    // there.
    let joints = skeleton.joints();
    let shoulder = screen.point(joints[arm]).unwrap();
    let inward = (shoulder - grab).normalized();
    let to = grab + inward * 14.0 + egui::vec2(-inward.y, inward.x) * 6.0;
    let target = on_plane(screen.ray(to), joint, screen.ray(grab).direction).unwrap();
    let reach = joints[arm].distance(joints[forearm]) + joints[forearm].distance(joint);
    assert!(joints[arm].distance(target) < reach * 0.98, "in reach");
    let (done, screen) = frame(&ctx, camera, &mut poser, &skeleton, &settings, at(to), NONE);
    let Some(BoneEvent::Pose(bones)) = done.event else {
        panic!("nothing was posed: {:?}", done.event);
    };
    let after = with(&pose, &bones);
    let moved = Skeleton {
        pose: &after,
        ..skeleton
    };
    let shown = screen.point(moved.joints()[hand]).unwrap();
    assert!(
        shown.distance(to) < 0.5,
        "the hand is under the mouse: {shown:?} vs {to:?}"
    );
    let turned: Vec<usize> = bones
        .iter()
        .filter(|(b, t)| differs(t, &pose[*b]))
        .map(|(b, _)| *b)
        .collect();
    let mut expected = vec![arm, forearm];
    expected.sort();
    assert_eq!(
        turned, expected,
        "the forearm and upper arm turned, nothing else"
    );
    assert_lengths_kept(rig, &pose, &after);
    frame(
        &ctx,
        camera,
        &mut poser,
        &skeleton,
        &settings,
        vec![press(false)],
        NONE,
    );
    assert!(poser.drag.is_none());
}

#[test]
fn chain_one_to_four_turns_that_many_bones_above_the_hand() {
    let model = mixamo();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let pose = rig.rest_pose();
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let bone = |name: &str| rig.bone(&format!("mixamorig:{name}")).unwrap();
    let hand = bone("LeftHand");
    let above = ["LeftForeArm", "LeftArm", "LeftShoulder", "Spine2"].map(bone);
    let target = skeleton.joints()[hand] + Vec3::new(0.1, 0.15, 0.1) * model.size[1];
    for chain in CHAIN_LENGTHS {
        let posed = skeleton.pose_with(Posing::Reach(hand, target), chain, &Pins::new());
        let mut expected = above[..chain].to_vec();
        expected.sort();
        assert_eq!(posed.changed, expected, "a chain of {chain}");
        assert_lengths_kept(rig, &pose, &posed.pose);
    }
}

#[test]
fn with_both_feet_pinned_moving_the_hips_down_bends_the_knees_forward_and_the_feet_stay() {
    let model = mixamo();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let pose = rig.rest_pose();
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let bone = |name: &str| rig.bone(&format!("mixamorig:{name}")).unwrap();
    let hips = bone("Hips");
    let legs = [
        (bone("LeftUpLeg"), bone("LeftLeg"), bone("LeftFoot")),
        (bone("RightUpLeg"), bone("RightLeg"), bone("RightFoot")),
    ];
    let mut poser = Poser {
        selected: Some(hips),
        tool: Tool::Move,
        ..Poser::default()
    };
    poser.for_clip("assets/animations/Walking/Crouch.anim.json");
    for (_, _, foot) in legs {
        poser.pin(&skeleton, foot);
    }
    let joints = skeleton.joints();
    let leg = joints[legs[0].0].distance(joints[legs[0].2]);
    let forward = skeleton.to_world().transform_vector3(Vec3::Z).normalize();
    // How far forward the knee sits off the line from hip to foot.
    let knee_out = |joints: &[Vec3], (hip, knee, foot): (usize, usize, usize)| {
        let line = (joints[foot] - joints[hip]).normalize();
        let off = joints[knee] - joints[hip];
        (off - line * off.dot(line)).dot(forward)
    };
    let settings = PoseSettings::default();
    let down = Posing::Reach(hips, joints[hips] - Vec3::Y * leg * 0.15);
    let posed = poser.pose(&skeleton, down, &settings);
    assert_eq!(posed.slipped, Vec::<usize>::new());
    let after = Skeleton {
        pose: &posed.pose,
        ..skeleton
    }
    .joints();
    assert!(after[hips].abs_diff_eq(joints[hips] - Vec3::Y * leg * 0.15, 1e-4));
    for leg_bones in legs {
        let (_, _, foot) = leg_bones;
        assert!(
            after[foot].distance(joints[foot]) < leg * 1e-3,
            "{} stays: {} moved",
            rig.bones[foot].name,
            after[foot].distance(joints[foot])
        );
        let (was, now) = (knee_out(&joints, leg_bones), knee_out(&after, leg_bones));
        assert!(
            now > was && now > leg * 0.05,
            "the knee bends forward: {was} to {now}"
        );
    }
    assert_lengths_kept(rig, &pose, &posed.pose);
    // The answer is the same every time, and the pins stay put.
    assert_eq!(poser.pose(&skeleton, down, &settings), posed);
    let pins: Vec<usize> = poser.pins().keys().copied().collect();
    assert_eq!(pins, [legs[0].2, legs[1].2]);

    // Lifting the hips out of the legs' reach can't keep the feet: it
    // says so rather than moving them quietly.
    let up = Posing::Reach(hips, joints[hips] + Vec3::Y * leg * 2.0);
    let posed = poser.pose(&skeleton, up, &settings);
    assert_eq!(posed.slipped, [legs[0].2, legs[1].2]);
    let message = slipped_message(rig, &posed.slipped);
    assert!(message.contains("mixamorig:LeftFoot") && message.contains("mixamorig:RightFoot"));
}

#[test]
fn pins_belong_to_one_clip() {
    let model = noodle();
    let rig = model.rig.as_ref().unwrap();
    let mut world = World::new();
    let part = part(&mut world, &model);
    let pose = rig.rest_pose();
    let skeleton = Skeleton::on(&world, part, rig, &pose, &[]).unwrap();
    let (hat, top) = (rig.bone("Hat").unwrap(), rig.bone("Top").unwrap());
    let mut poser = Poser::default();
    poser.for_clip("Wave.anim.json");
    poser.pin(&skeleton, hat);
    poser.for_clip("Bow.anim.json");
    assert!(poser.pins().is_empty(), "another clip has none");
    poser.pin(&skeleton, top);
    poser.for_clip("Wave.anim.json");
    assert_eq!(poser.pins().keys().copied().collect::<Vec<_>>(), [hat]);
    poser.unpin(hat);
    assert!(poser.pins().is_empty());
    poser.for_clip("Bow.anim.json");
    assert_eq!(poser.pins().keys().copied().collect::<Vec<_>>(), [top]);
}
