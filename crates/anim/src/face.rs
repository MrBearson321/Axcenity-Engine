//! Faces (facial animation F3): the standard facial controls turned into
//! a model's blend shape weights and eye and jaw bone turns, through a
//! face profile.
//!
//! A profile ([`FaceProfile`], `assets/faces/*.face.json` or a built-in
//! one) says which blend shapes each control drives. The eyes and the jaw
//! are bones when the model has them, found by name (`LeftEye`, `eye.L`,
//! `mixamorig:LeftEye`, `Jaw` …) or named by the profile: Eye_Look_* turn
//! the eye bones (and then not the look shapes), and Jaw_Open, Jaw_Left
//! and Jaw_Right turn the jaw bone as well as their shapes. Turns are
//! about the body's own axes (its left, up and forward, from the resting
//! skeleton), within the profile's limits, on top of whatever the body's
//! pose has the bone doing.
//!
//! [`FaceRig::bind`] matches a profile to a model once and reports what
//! doesn't fit ([`FaceProblem`]); [`FaceRig::weights`] and
//! [`FaceRig::turn`] then apply control values each frame.

use std::fmt;

use axcenity_assets::{FaceLimits, FaceProfile, FaceProfiles, Model, Pose, Posed, Rig};
use axcenity_core::face::{FACE_CONTROLS, face_control};
use glam::{Mat3, Quat, Vec3};

use crate::retarget::{HumanoidMap, Severity, Side, body_frame, name_key};

/// The built-in profiles' names, in the order a tie in fit goes to.
pub const BUILT_IN_FACE_PROFILES: &[&str] = &["axcenity", "arkit", "vroid"];

/// A built-in profile by name (any case): `axcenity` (blend shapes named
/// as the controls, as on Axcenity's own characters), `arkit` (Apple's 52
/// face tracking shapes, as Ready Player Me and many scanned heads have)
/// or `vroid` (VRoid Studio's `Fcl_*` shapes and eye bones).
pub fn built_in_face_profile(name: &str) -> Option<FaceProfile> {
    let read = |text: &str| FaceProfile::from_json(text).expect("built-in face profile");
    match name.to_ascii_lowercase().as_str() {
        "axcenity" => {
            let mut profile = FaceProfile::new();
            profile.name = "Axcenity".into();
            profile.controls = FACE_CONTROLS
                .iter()
                .map(|control| {
                    (
                        control.name.to_owned(),
                        vec![axcenity_assets::ShapeWeight::new(control.name, 1.0)],
                    )
                })
                .collect();
            Some(profile)
        }
        "arkit" => Some(read(include_str!("../faces/arkit.face.json"))),
        "vroid" => Some(read(include_str!("../faces/vroid.face.json"))),
        _ => None,
    }
}

/// The profile a face uses, and its name (a built-in's or a file's path):
/// `wanted` when it's set (a built-in's name or one of `project`'s paths),
/// otherwise the profile naming the most of the model's blend shapes, the
/// project's own before the built-ins, the first on a tie. A model none of
/// them names a shape of gets `axcenity` (its eye and jaw bones still turn).
pub fn choose_face_profile(
    model: &Model,
    project: &FaceProfiles,
    wanted: &str,
) -> Result<(String, FaceProfile), String> {
    let wanted = wanted.trim();
    if !wanted.is_empty() {
        if let Some((path, profile)) = project.profiles.iter().find(|(path, _)| path == wanted) {
            return Ok((path.clone(), profile.clone()));
        }
        if let Some(profile) = built_in_face_profile(wanted) {
            return Ok((wanted.to_ascii_lowercase(), profile));
        }
        return Err(format!(
            "there is no face profile `{wanted}` (built in: {}; or a `*.face.json` file's path)",
            BUILT_IN_FACE_PROFILES.join(", ")
        ));
    }
    let shapes = || {
        model
            .morph_targets
            .iter()
            .map(|target| target.name.as_str())
    };
    let built_in = BUILT_IN_FACE_PROFILES.iter().map(|name| {
        (
            name.to_string(),
            built_in_face_profile(name).expect("built in"),
        )
    });
    let mut best: Option<(String, FaceProfile, usize)> = None;
    for (name, profile) in project.profiles.iter().cloned().chain(built_in) {
        let fit = profile.fit(shapes());
        if fit > 0 && best.as_ref().is_none_or(|(_, _, most)| fit > *most) {
            best = Some((name, profile, fit));
        }
    }
    Ok(match best {
        Some((name, profile, _)) => (name, profile),
        None => (
            "axcenity".into(),
            built_in_face_profile("axcenity").expect("built in"),
        ),
    })
}

/// The bones a face turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FaceBoneRole {
    Jaw,
    LeftEye,
    RightEye,
}

impl FaceBoneRole {
    pub const ALL: [FaceBoneRole; 3] = [
        FaceBoneRole::Jaw,
        FaceBoneRole::LeftEye,
        FaceBoneRole::RightEye,
    ];

    /// Its name in a profile's `bones`.
    pub fn as_str(self) -> &'static str {
        match self {
            FaceBoneRole::Jaw => "Jaw",
            FaceBoneRole::LeftEye => "LeftEye",
            FaceBoneRole::RightEye => "RightEye",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == name)
    }

    /// Whether a bone named `name` plays it, going by the name alone.
    fn named_by(self, name: &str) -> bool {
        let (key, side) = name_key(name);
        match self {
            FaceBoneRole::Jaw => matches!(
                key.as_str(),
                "jaw" | "jawbone" | "lowerjaw" | "jawroot" | "mandible"
            ),
            FaceBoneRole::LeftEye | FaceBoneRole::RightEye => {
                let wanted = if self == FaceBoneRole::LeftEye {
                    Side::Left
                } else {
                    Side::Right
                };
                side == Some(wanted) && matches!(key.as_str(), "eye" | "eyeball" | "faceeye")
            }
        }
    }
}

/// Something about a face that won't work, or won't look right.
#[derive(Clone, Debug, PartialEq)]
pub enum FaceProblem {
    /// Standard controls that move nothing on this model (it needs only
    /// the ones it has).
    MissingControls(Vec<&'static str>),
    /// The profile maps a name that isn't a standard control.
    UnknownControl(String),
    /// The profile maps the same control twice; the first is used.
    RepeatedControl(String),
    /// A control names the same shape twice; their weights add up.
    RepeatedShape { control: String, shape: String },
    /// A control drives a blend shape the model doesn't have.
    MissingShape { control: String, shape: String },
    /// The profile names a bone the skeleton doesn't have.
    MissingBone { role: String, bone: String },
    /// The profile's `bones` has a name other than Jaw, LeftEye, RightEye.
    UnknownBoneRole(String),
    /// A weight that isn't a number from -1 to 1; it's left out.
    BadWeight {
        control: String,
        shape: String,
        weight: f32,
    },
    /// A limit that isn't 0 to 90 degrees; it's held to that.
    BadLimit { name: &'static str, degrees: f32 },
    /// No eye bones and no look shapes: the eyes can't look around.
    NoEyes,
    /// No jaw bone and no Jaw_Open shape: the jaw can't open.
    NoJaw,
}

impl FaceProblem {
    pub fn severity(&self) -> Severity {
        match self {
            FaceProblem::MissingControls(_) => Severity::Note,
            FaceProblem::RepeatedShape { .. }
            | FaceProblem::BadLimit { .. }
            | FaceProblem::NoEyes
            | FaceProblem::NoJaw => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

impl fmt::Display for FaceProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FaceProblem::MissingControls(controls) => write!(
                f,
                "{} of the {} standard controls move nothing: {}.",
                controls.len(),
                FACE_CONTROLS.len(),
                controls.join(", ")
            ),
            FaceProblem::UnknownControl(name) => write!(
                f,
                "`{name}` isn't a facial control, so it's left out (controls are named like Blink_Left and Jaw_Open)."
            ),
            FaceProblem::RepeatedControl(name) => {
                write!(f, "`{name}` is mapped twice; only the first is used.")
            }
            FaceProblem::RepeatedShape { control, shape } => {
                write!(f, "`{control}` names the shape `{shape}` twice.")
            }
            FaceProblem::MissingShape { control, shape } => write!(
                f,
                "`{control}` drives the blend shape `{shape}`, which the model doesn't have."
            ),
            FaceProblem::MissingBone { role, bone } => write!(
                f,
                "The {role} bone is `{bone}`, which the skeleton doesn't have."
            ),
            FaceProblem::UnknownBoneRole(role) => write!(
                f,
                "`{role}` in `bones` isn't Jaw, LeftEye or RightEye, so it's left out."
            ),
            FaceProblem::BadWeight {
                control,
                shape,
                weight,
            } => write!(
                f,
                "`{control}` drives `{shape}` by {weight}, not -1 to 1, so it's left out."
            ),
            FaceProblem::BadLimit { name, degrees } => write!(
                f,
                "The limit `{name}` is {degrees}°; limits are held to 0 to 90°."
            ),
            FaceProblem::NoEyes => f.write_str(
                "There are no eye bones (such as LeftEye and RightEye) and no look shapes, so the eyes can't look around.",
            ),
            FaceProblem::NoJaw => f.write_str(
                "There is no jaw bone (such as Jaw) and no Jaw_Open shape, so the jaw can't open.",
            ),
        }
    }
}

/// A bone the face turns.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TurnedBone {
    bone: usize,
    /// Its parent's resting turn in the file's space, so a turn about the
    /// body's axes can be made in the bone's own.
    parent_rest: Quat,
}

/// A profile matched to one model: which shapes each control drives and
/// which bones it turns.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceRig {
    /// The profile's name (a built-in's, or its file's path).
    pub profile: String,
    /// Each control's blend shapes (target index, weight), by control.
    drives: Vec<Vec<(u16, f32)>>,
    /// Whether any control drives each blend shape.
    driven: Vec<bool>,
    jaw: Option<TurnedBone>,
    eyes: [Option<TurnedBone>; 2],
    /// The body's left, up and forward, in the file's space.
    axes: Mat3,
    limits: FaceLimits,
    /// The head bone and its resting turn in the file's space, which gaze
    /// is measured from.
    head: Option<(usize, Quat)>,
}

const EYE_LOOK: [&str; 4] = [
    "Eye_Look_Up",
    "Eye_Look_Down",
    "Eye_Look_Left",
    "Eye_Look_Right",
];

impl FaceRig {
    /// Matches `profile` (called `name`) to `model`, saying what doesn't
    /// fit. What doesn't fit is left out; the rest works.
    pub fn bind(model: &Model, name: &str, profile: &FaceProfile) -> (FaceRig, Vec<FaceProblem>) {
        let mut problems = Vec::new();
        let target = |shape: &str| {
            model
                .morph_targets
                .iter()
                .position(|target| target.name == shape)
        };
        let mut drives: Vec<Vec<(u16, f32)>> = vec![Vec::new(); FACE_CONTROLS.len()];
        let mut mapped = vec![false; FACE_CONTROLS.len()];
        for (control, shapes) in &profile.controls {
            let Some(index) = face_control(control) else {
                problems.push(FaceProblem::UnknownControl(control.clone()));
                continue;
            };
            if mapped[index] {
                problems.push(FaceProblem::RepeatedControl(control.clone()));
                continue;
            }
            mapped[index] = true;
            for (at, drive) in shapes.iter().enumerate() {
                if shapes[..at]
                    .iter()
                    .any(|before| before.shape == drive.shape)
                {
                    problems.push(FaceProblem::RepeatedShape {
                        control: control.clone(),
                        shape: drive.shape.clone(),
                    });
                }
                if !drive.weight.is_finite() || drive.weight.abs() > 1.0 {
                    problems.push(FaceProblem::BadWeight {
                        control: control.clone(),
                        shape: drive.shape.clone(),
                        weight: drive.weight,
                    });
                    continue;
                }
                match target(&drive.shape) {
                    Some(target) => drives[index].push((target as u16, drive.weight)),
                    // Built-in profiles name every shape their kind of
                    // model can have; one a model lacks is just a control
                    // it doesn't have.
                    None if !BUILT_IN_FACE_PROFILES.contains(&name) => {
                        problems.push(FaceProblem::MissingShape {
                            control: control.clone(),
                            shape: drive.shape.clone(),
                        })
                    }
                    None => {}
                }
            }
        }

        for role in profile.bones.keys() {
            if FaceBoneRole::from_name(role).is_none() {
                problems.push(FaceProblem::UnknownBoneRole(role.clone()));
            }
        }
        let rig = model.rig.as_deref();
        let rest_world = rig.map(|rig| rig.world(&rig.rest_pose()));
        let built_in = BUILT_IN_FACE_PROFILES.contains(&name);
        let mut find = |role: FaceBoneRole| -> Option<TurnedBone> {
            let rig = rig?;
            let by_name = || rig.bones.iter().position(|bone| role.named_by(&bone.name));
            let bone = match profile.bones.get(role.as_str()) {
                Some(named) if named.is_empty() => return None,
                Some(named) => match rig.bone(named) {
                    Some(bone) => bone,
                    // A built-in profile's bone names are only its kind of
                    // model's; others are found by name.
                    None if built_in => by_name()?,
                    None => {
                        problems.push(FaceProblem::MissingBone {
                            role: role.as_str().into(),
                            bone: named.clone(),
                        });
                        return None;
                    }
                },
                None => by_name()?,
            };
            let parent_rest = rig.bones[bone]
                .parent
                .and_then(|parent| rest_world.as_ref()?.get(parent).copied())
                .map_or(Quat::IDENTITY, |matrix| {
                    matrix.to_scale_rotation_translation().1.normalize()
                });
            Some(TurnedBone { bone, parent_rest })
        };
        let jaw = find(FaceBoneRole::Jaw);
        let eyes = [find(FaceBoneRole::LeftEye), find(FaceBoneRole::RightEye)];

        let mut limits = profile.limits;
        for (name, degrees) in [
            ("look_sideways", &mut limits.look_sideways),
            ("look_up", &mut limits.look_up),
            ("look_down", &mut limits.look_down),
            ("jaw_open", &mut limits.jaw_open),
            ("jaw_sideways", &mut limits.jaw_sideways),
        ] {
            if !degrees.is_finite() || !(0.0..=90.0).contains(degrees) {
                problems.push(FaceProblem::BadLimit {
                    name,
                    degrees: *degrees,
                });
                *degrees = if degrees.is_finite() {
                    degrees.clamp(0.0, 90.0)
                } else {
                    0.0
                };
            }
        }

        let has_eyes = eyes.iter().any(Option::is_some);
        let drives_shape =
            |control: &str| face_control(control).is_some_and(|i| !drives[i].is_empty());
        if !has_eyes && !EYE_LOOK.iter().any(|control| drives_shape(control)) {
            problems.push(FaceProblem::NoEyes);
        }
        if jaw.is_none() && !drives_shape("Jaw_Open") {
            problems.push(FaceProblem::NoJaw);
        }
        let missing: Vec<&'static str> = FACE_CONTROLS
            .iter()
            .enumerate()
            .filter(|(index, control)| {
                let by_bone = (has_eyes && EYE_LOOK.contains(&control.name))
                    || (jaw.is_some()
                        && matches!(control.name, "Jaw_Open" | "Jaw_Left" | "Jaw_Right"));
                drives[*index].is_empty() && !by_bone
            })
            .map(|(_, control)| control.name)
            .collect();
        if !missing.is_empty() {
            problems.push(FaceProblem::MissingControls(missing));
        }

        let mut driven = vec![false; model.morph_targets.len()];
        for (target, _) in drives.iter().flatten() {
            driven[*target as usize] = true;
        }
        let axes = match (rig, &rest_world) {
            (Some(rig), Some(world)) => facing(rig, world, &eyes),
            _ => Mat3::IDENTITY,
        };
        let head = rig.zip(rest_world.as_ref()).and_then(|(rig, world)| {
            let head = HumanoidMap::detect(&rig.bones).get(crate::retarget::HumanBone::Head)?;
            Some((
                head,
                world[head].to_scale_rotation_translation().1.normalize(),
            ))
        });
        let face = FaceRig {
            profile: name.to_owned(),
            drives,
            driven,
            jaw,
            eyes,
            axes,
            limits,
            head,
        };
        (face, problems)
    }

    /// How many standard controls move something on the model.
    pub fn controls_used(&self) -> usize {
        (0..FACE_CONTROLS.len())
            .filter(|&index| self.moves(index))
            .count()
    }

    /// Whether the standard control at `index` moves something on the
    /// model: a blend shape, or an eye or jaw bone.
    pub fn moves(&self, index: usize) -> bool {
        let Some(control) = FACE_CONTROLS.get(index) else {
            return false;
        };
        !self.drives[index].is_empty()
            || (self.eyes_turn() && EYE_LOOK.contains(&control.name))
            || (self.jaw.is_some() && matches!(control.name, "Jaw_Open" | "Jaw_Left" | "Jaw_Right"))
    }

    /// The way the body faces, in the model file's space.
    pub fn forward(&self) -> Vec3 {
        self.axes.z_axis
    }

    fn eyes_turn(&self) -> bool {
        self.eyes.iter().any(Option::is_some)
    }

    /// The model's blend shape weights for `controls` (one value per
    /// standard control, 0 to 1), into `weights`: shapes no control drives
    /// keep the model's own values. With eye bones, Eye_Look_* turn the
    /// bones and not the look shapes.
    pub fn weights(&self, model: &Model, controls: &[f32], weights: &mut Vec<f32>) {
        weights.clear();
        weights.extend(
            model
                .morph_targets
                .iter()
                .zip(&self.driven)
                .map(|(target, &driven)| if driven { 0.0 } else { target.default_weight }),
        );
        for ((control, drives), &value) in FACE_CONTROLS.iter().zip(&self.drives).zip(controls) {
            let value = value.clamp(0.0, 1.0);
            if value == 0.0 || (self.eyes_turn() && EYE_LOOK.contains(&control.name)) {
                continue;
            }
            for &(target, weight) in drives {
                if let Some(at) = weights.get_mut(target as usize) {
                    *at += value * weight;
                }
            }
        }
    }

    /// Turns the eye and jaw bones in `pose` (the body's, done) by
    /// `controls`: about the body's own axes, within the profile's limits.
    pub fn turn(&self, pose: &mut Pose, controls: &[f32]) {
        let value = |name: &str| {
            face_control(name)
                .and_then(|index| controls.get(index))
                .map_or(0.0, |value| value.clamp(0.0, 1.0))
        };
        let [left, up, _] = self.axes.to_cols_array_2d().map(Vec3::from);
        let limits = &self.limits;
        // Toward the character's left is a positive turn about up; down is
        // a positive turn about left (its axes are right-handed).
        let turn = |yaw_degrees: f32, down_degrees: f32| {
            Quat::from_axis_angle(up, yaw_degrees.to_radians())
                * Quat::from_axis_angle(left, down_degrees.to_radians())
        };
        let look = turn(
            limits.look_sideways * (value("Eye_Look_Left") - value("Eye_Look_Right")),
            limits.look_down * value("Eye_Look_Down") - limits.look_up * value("Eye_Look_Up"),
        );
        let jaw = turn(
            limits.jaw_sideways * (value("Jaw_Left") - value("Jaw_Right")),
            limits.jaw_open * value("Jaw_Open"),
        );
        let bones = self
            .eyes
            .iter()
            .map(|eye| (eye, look))
            .chain([(&self.jaw, jaw)]);
        for (bone, turn) in bones {
            let Some(bone) = bone else {
                continue;
            };
            if turn == Quat::IDENTITY {
                continue;
            }
            if let Some(local) = pose.get_mut(bone.bone) {
                let own = bone.parent_rest.inverse() * turn * bone.parent_rest;
                local.rotation = (own * local.rotation).normalize();
            }
        }
    }
}

/// How far the eyes turn to look at a point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GazeAim {
    /// Eye_Look_Up, Eye_Look_Down, Eye_Look_Left and Eye_Look_Right, 0 to 1.
    pub look: [f32; 4],
    /// 1 while the point is in front, fading to 0 as it goes round behind.
    pub visible: f32,
}

impl FaceRig {
    /// The look controls that aim the eyes at `point` (in the model file's
    /// space) from where `pose` (the body's, or none for a model without a
    /// skeleton) has the eyes and head, plus `offset` degrees (sideways
    /// toward the left, up), held within the profile's limits.
    pub fn gaze(
        &self,
        rig: Option<&Rig>,
        pose: Option<&Pose>,
        point: Vec3,
        offset: glam::Vec2,
    ) -> GazeAim {
        let world = rig.zip(pose).map(|(rig, pose)| rig.world(pose));
        let at = |bone: usize| {
            world
                .as_ref()
                .and_then(|w| w.get(bone))
                .map(|m| m.w_axis.truncate())
        };
        let eyes: Vec<Vec3> = self
            .eyes
            .iter()
            .flatten()
            .filter_map(|eye| at(eye.bone))
            .collect();
        let from = if eyes.is_empty() {
            self.head
                .and_then(|(head, _)| at(head))
                .unwrap_or(Vec3::ZERO)
        } else {
            eyes.iter().sum::<Vec3>() / eyes.len() as f32
        };
        // The head's turn since rest carries the face's axes with it.
        let turned = self
            .head
            .and_then(|(head, rest)| {
                let now = world.as_ref()?.get(head)?.to_scale_rotation_translation().1;
                Some(now.normalize() * rest.inverse())
            })
            .unwrap_or(Quat::IDENTITY);
        let along = turned.inverse() * (point - from);
        let [left, up, forward] = self.axes.to_cols_array_2d().map(Vec3::from);
        let (x, y, z) = (along.dot(left), along.dot(up), along.dot(forward));
        if along.length_squared() < 1e-12 {
            return GazeAim::default();
        }
        let yaw = x.atan2(z).to_degrees() + offset.x;
        let pitch = y.atan2((x * x + z * z).sqrt()).to_degrees() + offset.y;
        GazeAim {
            look: self.look_from_degrees(glam::Vec2::new(yaw, pitch)),
            visible: ((130.0 - yaw.abs()) / 30.0).clamp(0.0, 1.0),
        }
    }

    /// The look controls (Up, Down, Left, Right) that turn the eyes
    /// `degrees` (sideways toward the left, up), within the profile's
    /// limits.
    pub fn look_from_degrees(&self, degrees: glam::Vec2) -> [f32; 4] {
        let limits = &self.limits;
        let share = |degrees: f32, limit: f32| {
            if limit > 0.0 {
                (degrees / limit).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        [
            share(degrees.y, limits.look_up),
            share(-degrees.y, limits.look_down),
            share(degrees.x, limits.look_sideways),
            share(-degrees.x, limits.look_sideways),
        ]
    }
}

/// Which way a resting skeleton faces, as its left, up and forward: from
/// its body when it's a humanoid, else from its eyes with Y up, else the
/// file's own axes (+X left, +Y up, +Z forward, as glTF characters face).
fn facing(rig: &Rig, world: &[glam::Mat4], eyes: &[Option<TurnedBone>; 2]) -> Mat3 {
    let map = HumanoidMap::detect(&rig.bones);
    if let Some(axes) = body_frame(world, &map) {
        return axes;
    }
    if let [Some(left_eye), Some(right_eye)] = eyes {
        let at = |bone: usize| world[bone].w_axis.truncate();
        let left = (at(left_eye.bone) - at(right_eye.bone)).normalize_or_zero();
        let forward = left.cross(Vec3::Y).normalize_or_zero();
        if forward != Vec3::ZERO {
            let up = forward.cross(left).normalize_or_zero();
            return Mat3::from_cols(left, up, forward);
        }
    }
    Mat3::IDENTITY
}

/// Checks `profile` against `model`: what [`FaceRig::bind`] reports.
pub fn check_face(model: &Model, name: &str, profile: &FaceProfile) -> Vec<FaceProblem> {
    FaceRig::bind(model, name, profile).1
}

impl FaceRig {
    /// The bone a view of the face is framed on: the head, else an eye,
    /// else the jaw.
    pub fn face_bone(&self) -> Option<usize> {
        self.head
            .map(|(bone, _)| bone)
            .or_else(|| self.eyes.iter().flatten().next().map(|eye| eye.bone))
            .or(self.jaw.map(|jaw| jaw.bone))
    }
}

/// `model` at rest with its face set to `controls` (one value per standard
/// control, 0 to 1): what Character Studio's preview draws.
pub fn pose_face(model: &Model, face: &FaceRig, controls: &[f32]) -> Posed {
    let mut posed = model
        .rig
        .as_ref()
        .and_then(|rig| {
            let mut pose = rig.rest_pose();
            face.turn(&mut pose, controls);
            model.pose(&pose)
        })
        .unwrap_or_else(|| model.rest_posed());
    let mut weights = Vec::with_capacity(model.morph_targets.len());
    face.weights(model, controls, &mut weights);
    posed.set_morph_weights(&weights);
    posed
}

/// Where the face is (its [`FaceRig::face_bone`] at rest), in the space a
/// MeshPart of size 1 at the origin draws `model` in; `None` without bones.
pub fn face_point(model: &Model, face: &FaceRig) -> Option<Vec3> {
    let rig = model.rig.as_ref()?;
    let bone = face.face_bone()?;
    let at = rig.world(&rig.rest_pose()).get(bone)?.w_axis.truncate();
    Some(rig.fit.transform_point3(at)).filter(|point| point.is_finite())
}
