//! Inverse kinematics on a posed rig: turning the bones above a joint so
//! the joint reaches a point.
//!
//! Everything is in the rig's file space (where [`Rig::world`] puts bones),
//! and only bone rotations change, so no bone gets longer or shorter. A
//! two-bone chain is solved exactly, bending the way it already bends (or,
//! if it's straight, the way it bends at rest). Longer chains use FABRIK.
//! Either way each bone is then swung onto its new direction the shortest
//! way, which keeps its twist. Out of reach, the chain points (all but)
//! straight at the target; it stops just short so it still knows which way
//! it bends next time.
//!
//! [`solve`] reaches several goals in order: a dragged joint, then pinned
//! ones. A goal's chain never has a bone above another goal's joint, so
//! reaching one goal can't move another, and the result doesn't depend on
//! anything but the order given.

use axcenity_assets::{BoneTransform, Rig};
use glam::{Quat, Vec3};

/// How far short of straight a chain stops when its target is out of
/// reach, as a share of its length.
const STRAIGHT: f32 = 1e-4;
/// A bone shorter than this share of its chain has no length to turn.
const TINY: f32 = 1e-6;
/// A goal is reached within this share of its chain's length.
const CLOSE: f32 = 1e-3;
const FABRIK_PASSES: usize = 32;

/// A joint (a bone's origin) to put at `target`, in the rig's file space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Goal {
    pub joint: usize,
    pub target: Vec3,
}

/// Where each bone's joint is in the rig's file space, for `pose`.
pub fn joint_positions(rig: &Rig, pose: &[BoneTransform]) -> Vec<Vec3> {
    rig.world(pose)
        .iter()
        .map(|bone| bone.w_axis.truncate())
        .collect()
}

/// Whether `bone` is under `above` in `rig` (not `above` itself).
pub fn is_below(rig: &Rig, bone: usize, above: usize) -> bool {
    let mut at = rig.bones.get(bone).and_then(|b| b.parent);
    while let Some(parent) = at {
        if parent == above {
            return true;
        }
        at = rig.bones[parent].parent;
    }
    false
}

/// The bones that turn to move `joint`: up to `length` named bones above
/// it, nearest first. Unnamed bones are passed over and don't count; the
/// chain stops at the first named bone that `can_turn` refuses.
pub fn chain(
    rig: &Rig,
    joint: usize,
    length: usize,
    can_turn: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let mut bones = Vec::new();
    let mut at = rig.bones.get(joint).and_then(|b| b.parent);
    while let Some(bone) = at
        && bones.len() < length
    {
        if !rig.bones[bone].name.is_empty() {
            if !can_turn(bone) {
                break;
            }
            bones.push(bone);
        }
        at = rig.bones[bone].parent;
    }
    bones
}

/// Reaches each goal in turn with a chain of up to `length` bones that
/// `can_turn` allows and that has no bone above another goal's joint.
/// Returns the joints of the goals it couldn't reach.
pub fn solve(
    rig: &Rig,
    pose: &mut [BoneTransform],
    goals: &[Goal],
    length: usize,
    can_turn: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let mut missed = Vec::new();
    for goal in goals {
        let others: Vec<usize> = goals
            .iter()
            .map(|other| other.joint)
            .filter(|&joint| joint != goal.joint)
            .collect();
        let bones = chain(rig, goal.joint, length, |bone| {
            can_turn(bone) && !others.iter().any(|&other| is_below(rig, other, bone))
        });
        if !reach(rig, pose, &bones, goal.joint, goal.target) {
            missed.push(goal.joint);
        }
    }
    missed
}

/// Turns `chain` (bones above `joint`, nearest first) so `joint` gets as
/// near `target` as it can. Returns whether it got there. A pose that
/// can't be worked out (nothing finite) is left as it was.
pub fn reach(
    rig: &Rig,
    pose: &mut [BoneTransform],
    chain: &[usize],
    joint: usize,
    target: Vec3,
) -> bool {
    reach_toward(rig, pose, chain, joint, target, None)
}

/// [`reach`], with the chain's middle joints swung around the line from its
/// top to its end so they sit on the side of `pole` (a point in the rig's
/// file space): an elbow or a knee points at it. With no pole, or a chain
/// lying straight, the limb bends the way it already bends.
pub fn reach_toward(
    rig: &Rig,
    pose: &mut [BoneTransform],
    chain: &[usize],
    joint: usize,
    target: Vec3,
    pole: Option<Vec3>,
) -> bool {
    let joints = joint_positions(rig, pose);
    let Some(&end) = joints.get(joint) else {
        return false;
    };
    // Top of the chain first, then the joint.
    let bones: Vec<usize> = chain.iter().rev().copied().collect();
    let nodes: Vec<Vec3> = bones.iter().map(|&b| joints[b]).chain([end]).collect();
    let lengths: Vec<f32> = nodes.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f32 = lengths.iter().sum();
    let close = |pose: &[BoneTransform]| {
        joint_positions(rig, pose)[joint].distance(target) <= total * CLOSE + 1e-6
    };
    if !target.is_finite() || total <= f32::EPSILON {
        return target.is_finite() && end.distance(target) <= 1e-6;
    }
    // Already there (a pin that holds): nothing turns.
    if end.distance(target) <= total * TINY {
        return true;
    }
    // A bone with no length can't move what's under it, so it sits out.
    let moving: Vec<usize> = (0..bones.len())
        .filter(|&i| lengths[i] > total * TINY)
        .collect();
    let points: Vec<Vec3> = moving.iter().map(|&i| nodes[i]).chain([end]).collect();
    let spans: Vec<f32> = moving.iter().map(|&i| lengths[i]).collect();
    let mut placed = match spans.len() {
        0 => return close(pose),
        1 => {
            let along = direction(target - points[0], points[1] - points[0], Vec3::X);
            vec![points[0], points[0] + along * spans[0]]
        }
        2 => {
            let hint = rest_bend(rig, pose, [bones[moving[0]], bones[moving[1]], joint]);
            two_bone(&points, spans[0], spans[1], target, hint)
        }
        _ => {
            let top = bones[moving[0]];
            let hint = rest_bend(rig, pose, [top, bones[moving[1]], joint]);
            fabrik(&points, &spans, target, hint)
        }
    };
    if let Some(pole) = pole {
        swing_to_pole(&mut placed, pole);
    }
    let before = pose.to_vec();
    for (step, &i) in moving.iter().enumerate() {
        let tip = moving.get(step + 1).map_or(joint, |&next| bones[next]);
        aim(rig, pose, bones[i], tip, placed[step + 1]);
    }
    if pose.iter().any(|bone| !bone.rotation.is_finite()) {
        pose.copy_from_slice(&before);
        return false;
    }
    close(pose)
}

/// `v` made unit length, or else `fallback`, or else `last`.
fn direction(v: Vec3, fallback: Vec3, last: Vec3) -> Vec3 {
    v.try_normalize()
        .or_else(|| fallback.try_normalize())
        .unwrap_or(last)
}

/// Which way the middle of three joints (`[top, middle, end]` bones) bends
/// at rest, turned as the top bone is turned now. Zero if it's straight
/// at rest too.
fn rest_bend(rig: &Rig, pose: &[BoneTransform], [top, middle, end]: [usize; 3]) -> Vec3 {
    let rest = rig.rest_pose();
    let (rest_world, now_world) = (rig.world(&rest), rig.world(pose));
    let at = |world: &[glam::Mat4], bone: usize| world[bone].w_axis.truncate();
    let line = at(&rest_world, end) - at(&rest_world, top);
    let mid = at(&rest_world, middle) - at(&rest_world, top);
    let Some(line) = line.try_normalize() else {
        return Vec3::ZERO;
    };
    let bend = mid - line * mid.dot(line);
    let turn = |world: &[glam::Mat4]| world[top].to_scale_rotation_translation().1;
    turn(&now_world) * turn(&rest_world).inverse() * bend
}

/// `hint` without its part along `axis` (unit), made unit length, or
/// `None` if it's (nearly) along it.
fn across(hint: Vec3, axis: Vec3, scale: f32) -> Option<Vec3> {
    let off = hint - axis * hint.dot(axis);
    (off.length() > scale * 1e-5).then(|| off.normalize())
}

/// Where a two-bone chain's three joints go (`points`: top, middle, end)
/// for its end to reach `target`: exactly, in the plane of its current
/// bend, or of `rest` when it's straight.
fn two_bone(points: &[Vec3], upper: f32, lower: f32, target: Vec3, rest: Vec3) -> Vec<Vec3> {
    let [top, middle, end] = [points[0], points[1], points[2]];
    let total = upper + lower;
    let line = direction(end - top, middle - top, Vec3::X);
    let axis = direction(target - top, line, Vec3::X);
    let longest = total * (1.0 - STRAIGHT);
    let shortest = ((upper - lower).abs() * (1.0 + STRAIGHT)).max(total * STRAIGHT);
    let reach = target.distance(top).clamp(shortest, longest.max(shortest));
    // Which way the middle joint sits off the line from top to end.
    let bend = (middle - top) - line * (middle - top).dot(line);
    let side = across(bend, axis, upper)
        .or_else(|| across(rest, axis, upper))
        .unwrap_or_else(|| axis.any_orthonormal_vector());
    let along = (reach * reach + upper * upper - lower * lower) / (2.0 * reach);
    let off = (upper * upper - along * along).max(0.0).sqrt();
    vec![top, top + axis * along + side * off, top + axis * reach]
}

/// Where a chain's joints go (`points`, top first) for its end to reach
/// `target`, by FABRIK: alternately pulling the end to the target and the
/// top back to where it was, keeping each bone's length.
fn fabrik(points: &[Vec3], lengths: &[f32], target: Vec3, rest: Vec3) -> Vec<Vec3> {
    let total: f32 = lengths.iter().sum();
    let top = points[0];
    let towards = direction(target - top, points[points.len() - 1] - top, Vec3::X);
    // Out of reach: just short of it, so the chain keeps a bend.
    let target = if target.distance(top) > total * (1.0 - STRAIGHT) {
        top + towards * total * (1.0 - STRAIGHT)
    } else {
        target
    };
    let mut points = points.to_vec();
    // A straight chain is given a slight bend (the way it bends at rest)
    // so it has a side to fold to.
    let line = direction(points[points.len() - 1] - top, towards, Vec3::X);
    let straight = points
        .iter()
        .all(|&p| ((p - top) - line * (p - top).dot(line)).length() <= total * 1e-4);
    if straight {
        let side = across(rest, line, total).unwrap_or_else(|| line.any_orthonormal_vector());
        let last = points.len() - 1;
        for point in &mut points[1..last] {
            *point += side * total * 1e-2;
        }
    }
    let was = points.clone();
    for _ in 0..FABRIK_PASSES {
        let last = points.len() - 1;
        points[last] = target;
        for i in (0..last).rev() {
            let along = direction(points[i] - points[i + 1], was[i] - was[i + 1], Vec3::X);
            points[i] = points[i + 1] + along * lengths[i];
        }
        points[0] = top;
        for i in 0..last {
            let along = direction(points[i + 1] - points[i], was[i + 1] - was[i], Vec3::X);
            points[i + 1] = points[i] + along * lengths[i];
        }
        if points[last].distance(target) <= total * 1e-5 {
            break;
        }
    }
    points
}

/// Turns a chain's middle joints about the line from its top to its end,
/// so they sit toward `pole`. Both ends stay where they are.
fn swing_to_pole(points: &mut [Vec3], pole: Vec3) {
    let last = points.len() - 1;
    let top = points[0];
    let Some(axis) = (points[last] - top).try_normalize() else {
        return;
    };
    if last < 2 {
        return;
    }
    let off_line = |point: Vec3| {
        let from_top = point - top;
        from_top - axis * from_top.dot(axis)
    };
    let sits: Vec3 = points[1..last].iter().map(|&point| off_line(point)).sum();
    let (Some(now), Some(wanted)) = (sits.try_normalize(), off_line(pole).try_normalize()) else {
        return;
    };
    let angle = axis.dot(now.cross(wanted)).atan2(now.dot(wanted));
    let turn = Quat::from_axis_angle(axis, angle);
    for point in &mut points[1..last] {
        *point = top + turn * (*point - top);
    }
}

/// Swings `bone` about its joint, the shortest way, so `tip` (a joint
/// under it) points at `to`.
fn aim(rig: &Rig, pose: &mut [BoneTransform], bone: usize, tip: usize, to: Vec3) {
    let world = rig.world(pose);
    let at = world[bone].w_axis.truncate();
    let (Some(from), Some(to)) = (
        (world[tip].w_axis.truncate() - at).try_normalize(),
        (to - at).try_normalize(),
    ) else {
        return;
    };
    turn_bone(rig, pose, &world, bone, Quat::from_rotation_arc(from, to));
}

/// Turns `bone` about its joint by `turn`, a rotation in the rig's file
/// space. `world` is [`Rig::world`] of `pose`.
pub(crate) fn turn_bone(
    rig: &Rig,
    pose: &mut [BoneTransform],
    world: &[glam::Mat4],
    bone: usize,
    turn: Quat,
) {
    // The turn in the world, seen from the parent's space.
    let parent = parent_turn(rig, world, bone);
    let along = parent.inverse() * turn * parent;
    pose[bone].rotation = (along * pose[bone].rotation).normalize();
}

/// How `bone`'s parent is turned in the rig's file space.
pub(crate) fn parent_turn(rig: &Rig, world: &[glam::Mat4], bone: usize) -> Quat {
    rig.bones[bone].parent.map_or(Quat::IDENTITY, |p| {
        world[p].to_scale_rotation_translation().1
    })
}

#[cfg(test)]
mod tests {
    use axcenity_assets::Bone;
    use glam::Mat4;

    use super::*;

    /// A rig of `(name, parent, offset from the parent, turn)` bones.
    fn rig(bones: &[(&str, Option<usize>, Vec3, Quat)]) -> Rig {
        Rig {
            bones: bones
                .iter()
                .map(|&(name, parent, translation, rotation)| Bone {
                    name: name.to_owned(),
                    parent,
                    rest: BoneTransform {
                        translation,
                        rotation,
                        scale: Vec3::ONE,
                    },
                })
                .collect(),
            joints: Vec::new(),
            fit: Mat4::IDENTITY,
        }
    }

    /// Shoulder at the origin, an upper arm along X, and a forearm bent 40°
    /// up at the elbow.
    fn arm() -> Rig {
        rig(&[
            ("Shoulder", None, Vec3::ZERO, Quat::IDENTITY),
            ("Elbow", Some(0), Vec3::X, Quat::from_rotation_z(0.7)),
            ("Hand", Some(1), Vec3::X, Quat::IDENTITY),
        ])
    }

    /// Every bone's length (its offset from its parent) in the world.
    fn lengths(rig: &Rig, pose: &[BoneTransform]) -> Vec<f32> {
        let joints = joint_positions(rig, pose);
        rig.bones
            .iter()
            .enumerate()
            .map(|(bone, info)| {
                info.parent
                    .map_or(0.0, |p| joints[p].distance(joints[bone]))
            })
            .collect()
    }

    fn assert_lengths_kept(rig: &Rig, pose: &[BoneTransform]) {
        for (before, after) in lengths(rig, &rig.rest_pose())
            .iter()
            .zip(lengths(rig, pose))
        {
            assert!((before - after).abs() < 1e-4, "{before} became {after}");
        }
        for (bone, (rest, now)) in rig.rest_pose().iter().zip(pose).enumerate() {
            assert_eq!(
                rest.translation, now.translation,
                "bone {bone} wasn't moved"
            );
            assert!(now.rotation.is_finite() && now.rotation.is_normalized());
        }
    }

    /// The way the middle joint bends: the normal of the plane it bends in.
    fn bend(rig: &Rig, pose: &[BoneTransform], [a, b, c]: [usize; 3]) -> Vec3 {
        let j = joint_positions(rig, pose);
        (j[b] - j[a]).cross(j[c] - j[b])
    }

    #[test]
    fn a_pole_decides_which_way_the_middle_joint_bends() {
        // The arm bends up (+Y); a pole below turns its elbow down instead.
        let arm = arm();
        let mut pose = arm.rest_pose();
        let target = Vec3::new(1.4, 0.0, 0.0);
        let pole = Some(Vec3::new(0.7, -5.0, 0.0));
        assert!(reach_toward(&arm, &mut pose, &[1, 0], 2, target, pole));
        let joints = joint_positions(&arm, &pose);
        assert!(joints[2].distance(target) < 1e-3, "the hand still arrives");
        assert!(joints[1].y < -0.1, "the elbow went down: {}", joints[1]);
        assert_lengths_kept(&arm, &pose);
    }

    #[test]
    fn two_bones_reach_a_target_exactly_and_keep_their_lengths() {
        let arm = arm();
        let mut pose = arm.rest_pose();
        let target = Vec3::new(1.2, 0.8, 0.3);
        assert!(reach(&arm, &mut pose, &[1, 0], 2, target));
        assert!(joint_positions(&arm, &pose)[2].distance(target) < 1e-4);
        assert_lengths_kept(&arm, &pose);
        assert_eq!(pose[2], arm.rest_pose()[2], "the hand itself isn't turned");
    }

    #[test]
    fn out_of_reach_the_chain_points_at_the_target_without_stretching() {
        let arm = arm();
        let mut pose = arm.rest_pose();
        let target = Vec3::new(10.0, 5.0, -2.0);
        assert!(
            !reach(&arm, &mut pose, &[1, 0], 2, target),
            "it can't get there"
        );
        let joints = joint_positions(&arm, &pose);
        let pointing = (joints[2] - joints[0]).normalize();
        assert!(
            pointing.angle_between(target.normalize()) < 1e-3,
            "{pointing}"
        );
        assert!((joints[2].length() - 2.0).abs() < 1e-3, "all but straight");
        assert_lengths_kept(&arm, &pose);
        // Longer chains too, and the next move still bends the same way.
        let long = rig(&[
            ("A", None, Vec3::ZERO, Quat::IDENTITY),
            ("B", Some(0), Vec3::X, Quat::from_rotation_z(0.3)),
            ("C", Some(1), Vec3::X, Quat::from_rotation_z(0.3)),
            ("D", Some(2), Vec3::X, Quat::IDENTITY),
        ]);
        let mut pose = long.rest_pose();
        let was = bend(&long, &pose, [0, 1, 3]);
        assert!(!reach(
            &long,
            &mut pose,
            &[2, 1, 0],
            3,
            Vec3::new(0.0, 20.0, 0.0)
        ));
        let joints = joint_positions(&long, &pose);
        assert!((joints[3] - joints[0]).normalize().angle_between(Vec3::Y) < 1e-2);
        assert_lengths_kept(&long, &pose);
        // Pulled straight back along itself, it folds the way it was bent.
        assert!(reach(
            &long,
            &mut pose,
            &[2, 1, 0],
            3,
            Vec3::new(0.0, 2.0, 0.0)
        ));
        assert!(
            bend(&long, &pose, [0, 1, 3]).dot(was) > 0.0,
            "it bent the same way"
        );
        assert_lengths_kept(&long, &pose);
    }

    #[test]
    fn moving_a_joint_again_and_again_never_flips_its_bend() {
        let arm = arm();
        // Round and round, in and out of reach, close to the shoulder.
        let target = |step: usize| {
            let t = step as f32 * 0.05;
            let radius = 1.5 + 1.1 * (t * 0.37).sin();
            Vec3::new(
                radius * t.cos(),
                radius * t.sin() * 0.8,
                0.4 * (t * 0.5).sin(),
            )
        };
        let mut pose = arm.rest_pose();
        reach(&arm, &mut pose, &[1, 0], 2, target(0));
        let mut last = bend(&arm, &pose, [0, 1, 2]);
        assert!(last.dot(bend(&arm, &arm.rest_pose(), [0, 1, 2])) > 0.0);
        let mut last_turns: Vec<Quat> = pose.iter().map(|b| b.rotation).collect();
        for step in 1..400 {
            reach(&arm, &mut pose, &[1, 0], 2, target(step));
            let now = bend(&arm, &pose, [0, 1, 2]);
            assert!(now.dot(last) > 0.0, "step {step} flipped: {last} to {now}");
            for (bone, (a, b)) in last_turns.iter().zip(&pose).enumerate() {
                let jump = a.angle_between(b.rotation);
                assert!(jump < 0.6, "step {step}: bone {bone} jumped {jump} rad");
            }
            assert_lengths_kept(&arm, &pose);
            last = now;
            last_turns = pose.iter().map(|b| b.rotation).collect();
        }
    }

    /// A spine of six bones, with an unnamed bone between the third and
    /// fourth, each 1 long and a little bent.
    fn spine() -> Rig {
        let bent = Quat::from_rotation_z(0.2);
        rig(&[
            ("B0", None, Vec3::ZERO, Quat::IDENTITY),
            ("B1", Some(0), Vec3::Y, bent),
            ("B2", Some(1), Vec3::Y, bent),
            ("", Some(2), Vec3::Y * 0.5, Quat::IDENTITY),
            ("B3", Some(3), Vec3::Y * 0.5, bent),
            ("B4", Some(4), Vec3::Y, bent),
            ("Tip", Some(5), Vec3::Y, Quat::IDENTITY),
        ])
    }

    #[test]
    fn chains_of_one_to_four_turn_those_bones_above_and_keep_every_length() {
        let spine = spine();
        let tip = 6;
        let expected: [&[usize]; 4] = [&[5], &[5, 4], &[5, 4, 2], &[5, 4, 2, 1]];
        for (length, bones) in (1..=4).zip(expected) {
            assert_eq!(
                chain(&spine, tip, length, |_| true),
                bones,
                "a chain of {length}"
            );
            let mut pose = spine.rest_pose();
            let start = joint_positions(&spine, &pose)[tip];
            reach(
                &spine,
                &mut pose,
                bones,
                tip,
                start + Vec3::new(0.3, -0.2, 0.2),
            );
            assert_lengths_kept(&spine, &pose);
            for (bone, (rest, now)) in spine.rest_pose().iter().zip(&pose).enumerate() {
                let turned = rest.rotation.angle_between(now.rotation) > 1e-6;
                assert_eq!(turned, bones.contains(&bone), "{length}: bone {bone}");
            }
        }
        // A bone that can't turn ends the chain there.
        assert_eq!(chain(&spine, tip, 4, |bone| bone != 2), [5, 4]);
        assert_eq!(
            chain(&spine, 0, 4, |_| true),
            Vec::<usize>::new(),
            "the root has none"
        );
    }

    #[test]
    fn short_bones_targets_on_the_joint_and_straight_chains_stay_finite_and_calm() {
        // A bone of no length in the chain sits out.
        let zero = rig(&[
            ("A", None, Vec3::ZERO, Quat::IDENTITY),
            ("B", Some(0), Vec3::X, Quat::from_rotation_z(0.5)),
            ("C", Some(1), Vec3::ZERO, Quat::IDENTITY),
            ("D", Some(2), Vec3::X, Quat::IDENTITY),
        ]);
        let mut pose = zero.rest_pose();
        assert!(reach(
            &zero,
            &mut pose,
            &[2, 1, 0],
            3,
            Vec3::new(1.0, 1.0, 0.0)
        ));
        assert_lengths_kept(&zero, &pose);
        // Nothing but bones of no length: nothing to turn, nothing broken.
        let point = rig(&[
            ("A", None, Vec3::ZERO, Quat::IDENTITY),
            ("B", Some(0), Vec3::ZERO, Quat::IDENTITY),
            ("C", Some(1), Vec3::ZERO, Quat::IDENTITY),
        ]);
        let mut pose = point.rest_pose();
        assert!(!reach(&point, &mut pose, &[1, 0], 2, Vec3::ONE));
        assert_eq!(pose, point.rest_pose());

        // The target where the joint already is: nothing turns.
        let arm = arm();
        let mut pose = arm.rest_pose();
        let hand = joint_positions(&arm, &pose)[2];
        assert!(reach(&arm, &mut pose, &[1, 0], 2, hand));
        for (rest, now) in arm.rest_pose().iter().zip(&pose) {
            assert!(rest.rotation.angle_between(now.rotation) < 1e-3);
        }
        // On the shoulder: folded up as far as it goes, still finite.
        reach(&arm, &mut pose, &[1, 0], 2, Vec3::ZERO);
        assert_lengths_kept(&arm, &pose);
        // A NaN target changes nothing.
        let before = pose.clone();
        assert!(!reach(&arm, &mut pose, &[1, 0], 2, Vec3::NAN));
        assert_eq!(pose, before);

        // Nearly straight (and dead straight, bending as it does at rest):
        // a small move makes a small turn.
        for nearly in [1e-6, 0.0] {
            let mut pose = arm.rest_pose();
            pose[1].rotation = Quat::from_rotation_z(nearly);
            let hand = joint_positions(&arm, &pose)[2];
            let before = pose.clone();
            assert!(reach(&arm, &mut pose, &[1, 0], 2, hand - Vec3::X * 1e-3));
            for (bone, (a, b)) in before.iter().zip(&pose).enumerate() {
                let turn = a.rotation.angle_between(b.rotation);
                assert!(turn < 0.1, "{nearly}: bone {bone} turned {turn}");
            }
            assert!(
                bend(&arm, &pose, [0, 1, 2]).z > 0.0,
                "it bends the way it does at rest"
            );
        }
    }

    /// Two arms on a spine: `Spine` at the origin, then a left and a right
    /// arm, each Shoulder, Elbow, Hand.
    fn two_arms() -> Rig {
        let bent = Quat::from_rotation_y(0.6);
        rig(&[
            ("Hips", None, Vec3::ZERO, Quat::IDENTITY),
            ("Spine", Some(0), Vec3::Y, Quat::IDENTITY),
            ("LeftShoulder", Some(1), Vec3::X * 0.3, Quat::IDENTITY),
            ("LeftElbow", Some(2), Vec3::X, bent),
            ("LeftHand", Some(3), Vec3::X, Quat::IDENTITY),
            ("RightShoulder", Some(1), -Vec3::X * 0.3, Quat::IDENTITY),
            ("RightElbow", Some(5), -Vec3::X, bent),
            ("RightHand", Some(6), -Vec3::X, Quat::IDENTITY),
        ])
    }

    #[test]
    fn pinned_joints_never_move_and_the_answer_is_always_the_same() {
        let rig = two_arms();
        let rest = rig.rest_pose();
        let joints = joint_positions(&rig, &rest);
        let (left_hand, right_hand, left_elbow) = (4, 7, 3);
        let drag = Goal {
            joint: left_hand,
            target: joints[left_hand] + Vec3::new(-0.4, 0.5, 0.3),
        };
        let pin = |joint: usize| Goal {
            joint,
            target: joints[joint],
        };
        // Chains of four would turn the spine and swing the other hand;
        // with it pinned, they stop below the spine.
        let solved = |goals: &[Goal]| {
            let mut pose = rest.clone();
            let missed = solve(&rig, &mut pose, goals, 4, |_| true);
            (pose, missed)
        };
        let (pose, missed) = solved(&[drag, pin(right_hand)]);
        assert_eq!(missed, Vec::<usize>::new());
        let now = joint_positions(&rig, &pose);
        assert_eq!(now[right_hand], joints[right_hand], "the pinned hand stays");
        assert!(now[left_hand].distance(drag.target) < 1e-3);
        assert_eq!(pose[1], rest[1], "the spine didn't turn");
        assert_eq!(
            solved(&[drag, pin(right_hand)]).0,
            pose,
            "the same every time"
        );

        // Pinning the left elbow too leaves only the forearm to turn the
        // hand with: the elbow stays and the hand gets as near as it can.
        let (pose, missed) = solved(&[drag, pin(right_hand), pin(left_elbow)]);
        let now = joint_positions(&rig, &pose);
        assert_eq!(now[left_elbow], joints[left_elbow]);
        assert_eq!(now[right_hand], joints[right_hand]);
        assert_eq!(missed, [left_hand], "the hand can't reach on its own");
        assert_lengths_kept(&rig, &pose);
        let turned: Vec<usize> = (0..rig.bones.len())
            .filter(|&b| pose[b].rotation.angle_between(rest[b].rotation) > 1e-6)
            .collect();
        assert_eq!(turned, [left_elbow]);

        // A pin out of reach of what may turn is reported, not hidden.
        let far = Goal {
            joint: right_hand,
            target: Vec3::new(-9.0, 0.0, 0.0),
        };
        assert_eq!(solved(&[far]).1, [right_hand]);
    }
}
