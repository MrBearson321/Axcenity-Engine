//! Checking a skeleton before retargeting to or from it: whether its bones
//! are matched to the roles retargeting needs, whether the matches make
//! sense (each below the one above it, left on the left), how it rests,
//! and whether two skeletons rest alike.

use axcenity_assets::{Bone, Rig, RigDefinition, RigDefinitions};
use glam::{Mat4, Vec3};

use super::roles::{HumanBone, HumanoidMap, Side};
use super::{body_frame, leg_length, position};

/// How much a [`RigProblem`] matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Retargeting can't work until it's fixed.
    Error,
    /// It works, but something will look wrong.
    Warning,
    /// Worth knowing.
    Note,
}

/// One thing a check found.
#[derive(Clone, Debug, PartialEq)]
pub struct RigProblem {
    pub severity: Severity,
    /// The role it's about, if it's about one.
    pub role: Option<HumanBone>,
    /// What's wrong and what to do, as one sentence.
    pub message: String,
}

impl RigProblem {
    fn new(severity: Severity, role: Option<HumanBone>, message: String) -> Self {
        Self {
            severity,
            role,
            message,
        }
    }
}

/// What [`check_rig`] found about one skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct RigReport {
    /// Which bone plays each role.
    pub map: HumanoidMap,
    /// The rig definition that fits it, if the project has one.
    pub definition: Option<String>,
    /// Worst first.
    pub problems: Vec<RigProblem>,
}

impl RigReport {
    /// Whether retargeting can use it (no errors).
    pub fn usable(&self) -> bool {
        !self
            .problems
            .iter()
            .any(|problem| problem.severity == Severity::Error)
    }
}

/// How far (degrees) a resting skeleton's upper arms point below level:
/// about 0 in a T-pose, 30 to 50 in an A-pose. `None` without both arms.
pub fn rest_arm_angle(bones: &[Bone], map: &HumanoidMap) -> Option<f32> {
    let world = rest_world(bones);
    let frame = body_frame(&world, map)?;
    let up = frame.y_axis;
    let at = |role| map.get(role).map(|bone| position(&world[bone]));
    let mut angles = Vec::new();
    for (upper, lower) in [
        (HumanBone::LeftUpperArm, HumanBone::LeftLowerArm),
        (HumanBone::RightUpperArm, HumanBone::RightLowerArm),
    ] {
        let (Some(from), Some(to)) = (at(upper), at(lower)) else {
            continue;
        };
        let along = (to - from).normalize_or_zero();
        if along != Vec3::ZERO {
            angles.push((-along.dot(up)).clamp(-1.0, 1.0).asin().to_degrees());
        }
    }
    (!angles.is_empty()).then(|| angles.iter().sum::<f32>() / angles.len() as f32)
}

/// Checks a skeleton (`bones`, parents first) for retargeting, matched
/// through `definition` if one is given, else through the project's best
/// fitting one in `rigs`, else by names.
pub fn check_rig(
    bones: &[Bone],
    definition: Option<&RigDefinition>,
    rigs: &RigDefinitions,
) -> RigReport {
    let (map, found) = match definition {
        Some(definition) => (HumanoidMap::with_definition(bones, definition), None),
        None => HumanoidMap::for_skeleton(bones, rigs),
    };
    let mut problems = Vec::new();
    let used = definition.or_else(|| {
        let path = found.as_deref()?;
        rigs.definitions
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, d)| d)
    });
    if let Some(definition) = used {
        check_definition(bones, definition, &mut problems);
    }

    let missing = map.missing();
    if !missing.is_empty() {
        let names: Vec<&str> = missing.iter().map(|role| role.name()).collect();
        problems.push(RigProblem::new(
            Severity::Error,
            missing.first().copied(),
            format!(
                "No bone plays {}: pick them in its bone map, or the skeleton can't be \
                 retargeted to or from.",
                names.join(", ")
            ),
        ));
    }

    // The same bone can't play two roles.
    let roles: Vec<(HumanBone, usize)> = map.roles().collect();
    for (i, &(role, bone)) in roles.iter().enumerate() {
        if let Some(&(first, _)) = roles[..i].iter().find(|&&(_, other)| other == bone) {
            problems.push(RigProblem::new(
                Severity::Error,
                Some(role),
                format!(
                    "`{}` plays both {} and {}; give one of them another bone.",
                    bones[bone].name,
                    first.name(),
                    role.name()
                ),
            ));
        }
    }

    // Each role's bone should hang below the bone of the role above it.
    for &(role, bone) in &roles {
        let mut above = role.above();
        while let Some(parent_role) = above {
            if let Some(parent) = map.get(parent_role) {
                if !is_below(bones, bone, parent) {
                    problems.push(RigProblem::new(
                        Severity::Warning,
                        Some(role),
                        format!(
                            "{} (`{}`) isn't below {} (`{}`) in the skeleton; check the bone map.",
                            role.name(),
                            bones[bone].name,
                            parent_role.name(),
                            bones[parent].name
                        ),
                    ));
                }
                break;
            }
            above = parent_role.above();
        }
    }

    let world = rest_world(bones);
    let at = |role| map.get(role).map(|bone| position(&world[bone]));

    // Left on the left: with the head up and the toes in front, the body's
    // left is up × forward.
    if let (Some(head), Some(hips), Some(left), Some(right)) = (
        at(HumanBone::Head),
        at(HumanBone::Hips),
        at(HumanBone::LeftUpperLeg),
        at(HumanBone::RightUpperLeg),
    ) {
        let forward = [HumanBone::LeftToes, HumanBone::RightToes]
            .into_iter()
            .zip([HumanBone::LeftFoot, HumanBone::RightFoot])
            .find_map(|(toes, foot)| Some(at(toes)? - at(foot)?));
        if let Some(forward) = forward {
            let up = head - hips;
            let forward = forward - up.normalize_or_zero() * forward.dot(up.normalize_or_zero());
            let leftward = up.cross(forward);
            if leftward.length_squared() > 1e-10 && (left - right).dot(leftward) < 0.0 {
                problems.push(RigProblem::new(
                    Severity::Warning,
                    Some(HumanBone::LeftUpperLeg),
                    "Its left and right look swapped (the left leg is on the body's right); \
                     check the bone map."
                        .to_owned(),
                ));
            }
        }
    }

    // A bone retargeting turns needs length to say which way it points.
    for role in [
        HumanBone::LeftUpperArm,
        HumanBone::LeftLowerArm,
        HumanBone::RightUpperArm,
        HumanBone::RightLowerArm,
        HumanBone::LeftUpperLeg,
        HumanBone::LeftLowerLeg,
        HumanBone::RightUpperLeg,
        HumanBone::RightLowerLeg,
    ] {
        let next = role.toward().into_iter().find_map(at);
        if let (Some(from), Some(to)) = (at(role), next)
            && from.distance(to) < 1e-5
        {
            problems.push(RigProblem::new(
                Severity::Warning,
                Some(role),
                format!(
                    "{} has no length (its next bone starts where it does), so which way \
                     it points can't be measured.",
                    role.name()
                ),
            ));
        }
    }

    // How it rests and its fingers only matter for a humanoid.
    let humanoid = missing.is_empty();
    if let Some(angle) = rest_arm_angle(bones, &map).filter(|_| humanoid) {
        let pose = if angle.abs() < 15.0 {
            "a T-pose".to_owned()
        } else {
            format!("an A-pose (arms {angle:.0}° below level)")
        };
        problems.push(RigProblem::new(
            Severity::Note,
            Some(HumanBone::LeftUpperArm),
            format!("It rests in {pose}."),
        ));
    }

    for side in [Side::Left, Side::Right].into_iter().filter(|_| humanoid) {
        let hand = HumanBone::hand(side);
        let count = map.finger_count(side);
        let which = match side {
            Side::Left => "left",
            Side::Right => "right",
        };
        if count == 0 {
            problems.push(RigProblem::new(
                Severity::Note,
                Some(hand),
                format!("The {which} hand has no finger bones matched; its fingers won't move."),
            ));
        } else if count < 15 {
            let missing: Vec<&str> = HumanBone::fingers(side)
                .filter(|&role| map.get(role).is_none())
                .map(HumanBone::name)
                .collect();
            problems.push(RigProblem::new(
                Severity::Note,
                Some(hand),
                format!(
                    "The {which} hand has {count} of 15 finger bones; no bone plays {}.",
                    missing.join(", ")
                ),
            ));
        }
    }

    problems.sort_by_key(|problem| problem.severity);
    RigReport {
        map,
        definition: found,
        problems,
    }
}

/// What a rig definition gets wrong for `bones`: role names it doesn't
/// know, and bones the skeleton doesn't have.
fn check_definition(bones: &[Bone], definition: &RigDefinition, problems: &mut Vec<RigProblem>) {
    for (role, bone) in &definition.bones {
        let known = HumanBone::from_name(role);
        if known.is_none() {
            problems.push(RigProblem::new(
                Severity::Warning,
                None,
                format!("Its rig definition names a role `{role}` there's no such role as."),
            ));
        }
        if !bone.is_empty() && !bones.iter().any(|b| &b.name == bone) {
            problems.push(RigProblem::new(
                Severity::Error,
                known,
                format!(
                    "Its rig definition gives {role} the bone `{bone}`, which it doesn't have."
                ),
            ));
        }
    }
}

/// What two skeletons that a take goes between differ in, beyond each
/// one's own [`check_rig`]: the source's (the take's) and the target's.
pub fn check_pair(source: &[Bone], target: &[Bone], rigs: &RigDefinitions) -> Vec<RigProblem> {
    let (from, _) = HumanoidMap::for_skeleton(source, rigs);
    let (to, _) = HumanoidMap::for_skeleton(target, rigs);
    let mut problems = Vec::new();
    if let (Some(a), Some(b)) = (rest_arm_angle(source, &from), rest_arm_angle(target, &to))
        && (a - b).abs() >= 10.0
    {
        problems.push(RigProblem::new(
            Severity::Warning,
            Some(HumanBone::LeftUpperArm),
            format!(
                "They rest differently: the take's arms are {a:.0}° below level and the \
                 model's {b:.0}°, so the arms will be off by {:.0}° unless Match rest poses \
                 is on.",
                (a - b).abs()
            ),
        ));
    }
    for side in [Side::Left, Side::Right] {
        let (a, b) = (from.finger_count(side), to.finger_count(side));
        if a > 0 && b > 0 && a != b {
            problems.push(RigProblem::new(
                Severity::Note,
                Some(HumanBone::hand(side)),
                format!(
                    "The take has {a} finger bones on that hand and the model {b}; only the \
                     ones both have move."
                ),
            ));
        } else if a > 0 && b == 0 {
            problems.push(RigProblem::new(
                Severity::Note,
                Some(HumanBone::hand(side)),
                "The take moves fingers the model has no bones for.".to_owned(),
            ));
        }
    }
    let (source_world, target_world) = (rest_world(source), rest_world(target));
    if let (Some(a), Some(b)) = (
        leg_length(&source_world, &from),
        leg_length(&target_world, &to),
    ) && a > 1e-6
    {
        let ratio = b / a;
        if !(0.8..=1.25).contains(&ratio) {
            problems.push(RigProblem::new(
                Severity::Note,
                Some(HumanBone::Hips),
                format!(
                    "The model's legs are {ratio:.2} times as long as the take's; the hips' \
                     travel is scaled to match."
                ),
            ));
        }
    }
    problems.sort_by_key(|problem| problem.severity);
    problems
}

fn rest_world(bones: &[Bone]) -> Vec<Mat4> {
    let rig = Rig {
        bones: bones.to_vec(),
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    };
    rig.world(&rig.rest_pose())
}

/// Whether `bone` is `above` or hangs somewhere below it.
fn is_below(bones: &[Bone], bone: usize, above: usize) -> bool {
    let mut at = Some(bone);
    while let Some(index) = at {
        if index == above {
            return true;
        }
        at = bones.get(index).and_then(|b| b.parent);
    }
    false
}
