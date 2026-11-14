//! A face over time (facial animation F4): expressions blending in and
//! out, the layer stack, and the face's own motion (blinks, gaze, idle).
//!
//! Each frame a face's controls are built up layer by layer, in its
//! `LayerOrder`, lowest first: Base (the FaceControls' own properties),
//! Expression (`SetExpression`'s, blended in priority order), Dialogue
//! (lip sync, F5), Procedural (blinks, gaze and idle motion, worked out
//! from what's under them) and Override (`SetOverride`'s). Each layer
//! covers some controls by some amount; an Override layer replaces what's
//! under it by that amount times its weight and mask, an Additive one adds
//! to it.

use std::sync::OnceLock;

use axcenity_assets::{FaceExpression, FaceExpressions, Pose, Rig, expression_name};
use axcenity_core::face::{
    ExpressionPlay, FACE_CONTROLS, FaceBlend, FaceLayer, FaceLayerSetting, FaceState, face_control,
};
use glam::{Vec2, Vec3};

use crate::face::FaceRig;

/// The built-in expressions, in Paul's spec's order.
pub const BUILT_IN_EXPRESSIONS: &[&str] = &[
    "Neutral",
    "Happy",
    "Sad",
    "Angry",
    "Surprised",
    "Confused",
    "Worried",
    "Excited",
    "Frustrated",
    "Disgusted",
    "Fearful",
    "Laughing",
    "Crying",
    "Determined",
    "Sleepy",
];

fn built_ins() -> &'static [(&'static str, FaceExpression)] {
    static BUILT_INS: OnceLock<Vec<(&'static str, FaceExpression)>> = OnceLock::new();
    BUILT_INS.get_or_init(|| {
        macro_rules! preset {
            ($name:literal) => {
                (
                    $name,
                    FaceExpression::from_json(include_str!(concat!(
                        "../faces/expressions/",
                        $name,
                        ".expression.json"
                    )))
                    .expect("built-in expression"),
                )
            };
        }
        vec![
            preset!("Neutral"),
            preset!("Happy"),
            preset!("Sad"),
            preset!("Angry"),
            preset!("Surprised"),
            preset!("Confused"),
            preset!("Worried"),
            preset!("Excited"),
            preset!("Frustrated"),
            preset!("Disgusted"),
            preset!("Fearful"),
            preset!("Laughing"),
            preset!("Crying"),
            preset!("Determined"),
            preset!("Sleepy"),
        ]
    })
}

/// A built-in expression by name (any case).
pub fn built_in_expression(name: &str) -> Option<&'static FaceExpression> {
    built_ins()
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name))
        .map(|(_, expression)| expression)
}

/// The expression `name` names: the project's (by name or path) before a
/// built-in one.
pub fn find_expression<'a>(project: &'a FaceExpressions, name: &str) -> Option<&'a FaceExpression> {
    project
        .find(name)
        .map(|(_, expression)| expression)
        .or_else(|| built_in_expression(name))
}

/// Every expression a project can use by name: its own, then the
/// built-in ones it doesn't replace.
pub fn expression_names(project: &FaceExpressions) -> Vec<String> {
    let mut names: Vec<String> = project
        .expressions
        .iter()
        .map(|(path, expression)| expression_name(path, expression).to_owned())
        .collect();
    for name in BUILT_IN_EXPRESSIONS {
        if !names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
            names.push((*name).to_owned());
        }
    }
    names
}

/// One layer's say over each control: how much it covers it (0 to 1) and
/// its value times that ("premultiplied").
#[derive(Clone, Debug, PartialEq)]
pub struct LayerValues {
    pub value: Vec<f32>,
    pub cover: Vec<f32>,
}

impl LayerValues {
    pub fn empty() -> Self {
        Self {
            value: vec![0.0; FACE_CONTROLS.len()],
            cover: vec![0.0; FACE_CONTROLS.len()],
        }
    }

    /// Sets `control` to `value` over what's covered so far, by `amount`.
    fn cover(&mut self, control: usize, value: f32, amount: f32) {
        let amount = amount.clamp(0.0, 1.0);
        self.value[control] = self.value[control] * (1.0 - amount) + value * amount;
        self.cover[control] = self.cover[control] * (1.0 - amount) + amount;
    }
}

/// Eases 0 to 1 in and out.
fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Moves `value` toward `target` by `step`.
pub(crate) fn approach(value: f64, target: f64, step: f64) -> f64 {
    if (target - value).abs() <= step {
        target
    } else {
        value + step * (target - value).signum()
    }
}

/// Moves `state`'s expressions on by `dt` seconds (dropping those that
/// have faded out) and returns the Expression layer. An expression that
/// can't be found is dropped and its name added to `unknown`.
pub(crate) fn advance_expressions(
    state: &mut FaceState,
    project: &FaceExpressions,
    dt: f64,
    unknown: &mut Vec<String>,
) -> LayerValues {
    let mut layer = LayerValues::empty();
    state.expressions.retain(|play| {
        let found = find_expression(project, &play.name).is_some();
        if !found {
            unknown.push(play.name.clone());
        }
        found
    });
    for play in &mut state.expressions {
        let expression = find_expression(project, &play.name).expect("kept above");
        let seconds = play.transition.unwrap_or(expression.transition as f64);
        let step = if seconds > 0.0 { dt / seconds } else { 1.0 };
        play.presence = approach(play.presence, play.target_presence, step);
        play.intensity = approach(
            play.intensity,
            play.target_intensity,
            step * play.target_intensity.abs().max(1.0),
        );
    }
    state
        .expressions
        .retain(|play| play.presence > 0.0 || play.target_presence > 0.0);
    let mut plays: Vec<&ExpressionPlay> = state.expressions.iter().collect();
    plays.sort_by_key(|play| (play.priority, play.order));
    for play in plays {
        let expression = find_expression(project, &play.name).expect("kept above");
        let amount = (smooth(play.presence) * play.weight.clamp(0.0, 1.0)) as f32;
        if amount <= 0.0 {
            continue;
        }
        let intensity = (play.intensity * expression.intensity as f64) as f32;
        for (name, &value) in &expression.controls {
            let Some(control) = face_control(name) else {
                continue;
            };
            let side = side_scale(FACE_CONTROLS[control].name, play.asymmetry as f32);
            if side > 0.0 {
                layer.cover(control, value * intensity * side, amount);
            }
        }
    }
    layer
}

/// How much of a control `asymmetry` keeps: -1 keeps only the left side's
/// controls, 1 only the right side's, 0 both.
fn side_scale(control: &str, asymmetry: f32) -> f32 {
    let asymmetry = asymmetry.clamp(-1.0, 1.0);
    if control.ends_with("_Left") {
        1.0 - asymmetry.max(0.0)
    } else if control.ends_with("_Right") {
        1.0 + asymmetry.min(0.0)
    } else {
        1.0
    }
}

/// Where the face's own motion has got to this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Procedural {
    /// How closed each eye is by blinking, left then right.
    pub blink: [f32; 2],
    /// A point to look at in the model file's space, and how much.
    pub gaze: Option<(Vec3, f32)>,
    /// Idle eye darts, in degrees (toward the left, up).
    pub saccade: Vec2,
    /// Idle brow drift, added to both brows' raise.
    pub brow: f32,
}

/// Everything a face's controls are made from this frame.
#[derive(Clone, Debug)]
pub(crate) struct FaceLayers {
    pub base: Vec<f32>,
    pub expression: LayerValues,
    /// Lip sync (F5).
    pub dialogue: LayerValues,
    pub procedural: Procedural,
    pub overrides: Vec<(usize, f32)>,
    pub order: Vec<FaceLayer>,
    pub settings: Vec<FaceLayerSetting>,
}

impl FaceLayers {
    /// The face's controls: every layer over the ones under it. `rig` and
    /// `pose` (the body's, done) aim the gaze.
    pub fn compose(&self, face: &FaceRig, rig: Option<&Rig>, pose: Option<&Pose>) -> Vec<f32> {
        let mut out = vec![0.0; FACE_CONTROLS.len()];
        for &layer in &self.order {
            let setting = &self.settings[layer as usize];
            let values = match layer {
                FaceLayer::Base => LayerValues {
                    value: self.base.clone(),
                    cover: vec![1.0; FACE_CONTROLS.len()],
                },
                FaceLayer::Expression => self.expression.clone(),
                FaceLayer::Dialogue => self.dialogue.clone(),
                FaceLayer::Procedural => self.procedural(&out, face, rig, pose),
                FaceLayer::Override => {
                    let mut values = LayerValues::empty();
                    for &(control, value) in &self.overrides {
                        values.cover(control, value, 1.0);
                    }
                    values
                }
            };
            blend(&mut out, &values, setting);
        }
        for value in &mut out {
            *value = value.clamp(0.0, 1.0);
        }
        out
    }

    fn procedural(
        &self,
        below: &[f32],
        face: &FaceRig,
        rig: Option<&Rig>,
        pose: Option<&Pose>,
    ) -> LayerValues {
        let motion = &self.procedural;
        let mut values = LayerValues::empty();
        let index = |name| face_control(name).expect("a standard control");
        for (eye, name) in ["Blink_Left", "Blink_Right"].into_iter().enumerate() {
            if motion.blink[eye] > 0.0 {
                values.cover(index(name), 1.0, motion.blink[eye]);
            }
        }
        let looks = [
            "Eye_Look_Up",
            "Eye_Look_Down",
            "Eye_Look_Left",
            "Eye_Look_Right",
        ]
        .map(index);
        match motion.gaze {
            Some((point, amount)) if amount > 0.0 => {
                let aim = face.gaze(rig, pose, point, motion.saccade);
                let amount = amount * aim.visible;
                for (control, look) in looks.into_iter().zip(aim.look) {
                    values.cover(control, look, amount);
                }
            }
            _ if motion.saccade != Vec2::ZERO => {
                // Darts over whatever the controls have the eyes doing.
                let dart = face.look_from_degrees(motion.saccade);
                let net = [
                    below[looks[0]] - below[looks[1]] + dart[0] - dart[1],
                    below[looks[2]] - below[looks[3]] + dart[2] - dart[3],
                ];
                values.cover(looks[0], net[0].max(0.0), 1.0);
                values.cover(looks[1], (-net[0]).max(0.0), 1.0);
                values.cover(looks[2], net[1].max(0.0), 1.0);
                values.cover(looks[3], (-net[1]).max(0.0), 1.0);
            }
            _ => {}
        }
        if motion.brow != 0.0 {
            for name in ["Brow_Raise_Left", "Brow_Raise_Right"] {
                let control = index(name);
                values.cover(control, below[control] + motion.brow, 1.0);
            }
        }
        values
    }
}

/// `values` over `out`, by `setting`'s weight, blend and mask.
fn blend(out: &mut [f32], values: &LayerValues, setting: &FaceLayerSetting) {
    let weight = setting.weight.clamp(0.0, 1.0) as f32;
    if weight == 0.0 {
        return;
    }
    for (control, out) in out.iter_mut().enumerate() {
        let masked = setting
            .mask
            .as_ref()
            .is_some_and(|mask| !mask.contains(&control));
        if masked {
            continue;
        }
        let value = values.value[control] * weight;
        match setting.blend {
            FaceBlend::Override => {
                let cover = values.cover[control] * weight;
                *out = *out * (1.0 - cover) + value;
            }
            FaceBlend::Additive => *out += value,
        }
    }
}

/// How much of a face is worked out (facial LOD, F7), by how far it is
/// from the camera and whether it's in view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FaceLod {
    /// Every frame, everything.
    #[default]
    Full,
    /// 15 times a second, without idle motion.
    Reduced,
    /// 4 times a second, only the controls and expressions (no
    /// blinking, gaze, idle motion or lip sync shown).
    Static,
    /// Not shown: it rests, and its expressions and lip sync only keep
    /// time, twice a second.
    Off,
}

impl FaceLod {
    pub const ALL: [FaceLod; 4] = [
        FaceLod::Full,
        FaceLod::Reduced,
        FaceLod::Static,
        FaceLod::Off,
    ];

    /// The level for a face `distance` studs from the camera (`None`
    /// without one) with LodDistance `lod_distance` (0 or less: always
    /// whole), `in_view` or not.
    pub fn of(distance: Option<f32>, lod_distance: f32, in_view: bool) -> Self {
        let Some(distance) = distance.filter(|_| lod_distance > 0.0) else {
            return FaceLod::Full;
        };
        if !in_view || distance >= lod_distance * 4.0 {
            FaceLod::Off
        } else if distance >= lod_distance * 2.0 {
            FaceLod::Static
        } else if distance >= lod_distance {
            FaceLod::Reduced
        } else {
            FaceLod::Full
        }
    }

    /// Seconds between updates at this level (0: every frame).
    pub fn interval(self) -> f64 {
        match self {
            FaceLod::Full => 0.0,
            FaceLod::Reduced => 1.0 / 15.0,
            FaceLod::Static => 0.25,
            FaceLod::Off => 0.5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FaceLod::Full => "Full",
            FaceLod::Reduced => "Reduced",
            FaceLod::Static => "Expressions only",
            FaceLod::Off => "Off",
        }
    }
}

/// What a face works out this update: its procedural motion (blinks,
/// gaze, idle) and idle motion, and its lip sync, each on or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FaceDetail {
    pub procedural: bool,
    pub idle: bool,
    pub dialogue: bool,
}

impl FaceDetail {
    /// At `lod`, with the FaceControls' ProceduralEnabled `procedural`.
    pub fn at(lod: FaceLod, procedural: bool) -> Self {
        let shown = matches!(lod, FaceLod::Full | FaceLod::Reduced);
        Self {
            procedural: procedural && shown,
            idle: procedural && lod == FaceLod::Full,
            dialogue: shown,
        }
    }
}

/// A face's blink settings, from its FaceControls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BlinkSettings {
    pub enabled: bool,
    /// Which eyes: left, right.
    pub eyes: [bool; 2],
    pub interval: (f64, f64),
    pub duration: f64,
    pub double_chance: f64,
}

/// A face's own motion between frames: blinks, idle darts, the gaze it's
/// following.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FaceMotion {
    random: u64,
    /// Seconds to the next blink, and how far into the one going on.
    blink_wait: f64,
    blinking: Option<f64>,
    /// The blink going on is followed by another.
    double: bool,
    gaze_point: Option<Vec3>,
    gaze_amount: f32,
    saccade: Vec2,
    saccade_target: Vec2,
    saccade_wait: f64,
    time: f64,
}

impl FaceMotion {
    /// A face's motion, its randomness seeded by `seed` (so two faces don't
    /// blink together, and a face blinks the same way every run).
    pub fn new(seed: u64) -> Self {
        let mut motion = Self {
            random: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            blink_wait: 0.0,
            blinking: None,
            double: false,
            gaze_point: None,
            gaze_amount: 0.0,
            saccade: Vec2::ZERO,
            saccade_target: Vec2::ZERO,
            saccade_wait: 0.0,
            time: 0.0,
        };
        motion.blink_wait = motion.next() * 3.0;
        motion.saccade_wait = motion.next() * 2.0;
        motion
    }

    /// 0 to 1 (xorshift64*).
    fn next(&mut self) -> f64 {
        self.random ^= self.random >> 12;
        self.random ^= self.random << 25;
        self.random ^= self.random >> 27;
        let bits = self.random.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        bits as f64 / (1u64 << 53) as f64
    }

    /// Moves the blinks on by `dt` and says how closed each eye is.
    pub fn blink(&mut self, settings: &BlinkSettings, dt: f64) -> [f32; 2] {
        if !settings.enabled {
            self.blinking = None;
            return [0.0; 2];
        }
        let duration = settings.duration.max(0.02);
        let amount = match self.blinking {
            Some(time) => {
                let time = time + dt;
                if time >= duration {
                    self.blinking = None;
                    let (low, high) = settings.interval;
                    let (low, high) = (low.max(0.05), high.max(low.max(0.05)));
                    self.blink_wait = if std::mem::take(&mut self.double) {
                        0.08
                    } else {
                        low + (high - low) * self.next()
                    };
                    0.0
                } else {
                    self.blinking = Some(time);
                    // Closing takes 40% of the blink, opening the rest.
                    let closing = duration * 0.4;
                    if time < closing {
                        smooth(time / closing)
                    } else {
                        1.0 - smooth((time - closing) / (duration - closing))
                    }
                }
            }
            None => {
                self.blink_wait -= dt;
                if self.blink_wait <= 0.0 {
                    self.blinking = Some(0.0);
                    self.double =
                        !self.double && self.next() < settings.double_chance.clamp(0.0, 1.0);
                }
                0.0
            }
        } as f32;
        [
            if settings.eyes[0] { amount } else { 0.0 },
            if settings.eyes[1] { amount } else { 0.0 },
        ]
    }

    /// Moves the idle motion on by `dt` at `amount` (0 to 1): eye darts in
    /// degrees, and the brows' drift.
    pub fn idle(&mut self, amount: f32, dt: f64) -> (Vec2, f32) {
        self.time += dt;
        let amount = amount.clamp(0.0, 1.0);
        if amount == 0.0 {
            self.saccade = Vec2::ZERO;
            return (Vec2::ZERO, 0.0);
        }
        self.saccade_wait -= dt;
        if self.saccade_wait <= 0.0 {
            self.saccade_wait = 0.6 + 1.9 * self.next();
            self.saccade_target = Vec2::new(
                (self.next() as f32 * 2.0 - 1.0) * 6.0,
                (self.next() as f32 * 2.0 - 1.0) * 3.0,
            );
        }
        // A dart takes about 40 ms.
        let follow = 1.0 - (-dt as f32 / 0.04).exp();
        self.saccade += (self.saccade_target - self.saccade) * follow;
        let drift = 0.04 * ((self.time * 0.7).sin() + 0.5 * (self.time * 1.9 + 1.3).sin()) as f32;
        (self.saccade * amount, drift * amount)
    }

    /// Follows `target` (a point in the model file's space, or none) with
    /// `smoothing` seconds of lag, fading in and out to `weight`. Returns
    /// where the eyes look and how much.
    pub fn gaze(
        &mut self,
        target: Option<Vec3>,
        weight: f32,
        smoothing: f64,
        dt: f64,
    ) -> Option<(Vec3, f32)> {
        let wanted = if target.is_some() {
            weight.clamp(0.0, 1.0)
        } else {
            0.0
        };
        // A quarter of a second to look at something or away.
        self.gaze_amount = approach(self.gaze_amount as f64, wanted as f64, dt * 4.0) as f32;
        if let Some(target) = target {
            let follow = if smoothing > 0.0 {
                1.0 - (-dt / smoothing).exp() as f32
            } else {
                1.0
            };
            let point = self.gaze_point.get_or_insert(target);
            *point += (target - *point) * follow;
        }
        if self.gaze_amount <= 0.0 {
            self.gaze_point = None;
            return None;
        }
        self.gaze_point.map(|point| (point, self.gaze_amount))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_faces_lod_follows_its_distance_and_view() {
        let lod = |distance, in_view| FaceLod::of(distance, 10.0, in_view);
        assert_eq!(lod(None, false), FaceLod::Full, "no camera: whole");
        assert_eq!(lod(Some(5.0), true), FaceLod::Full);
        assert_eq!(lod(Some(10.0), true), FaceLod::Reduced);
        assert_eq!(lod(Some(25.0), true), FaceLod::Static);
        assert_eq!(lod(Some(40.0), true), FaceLod::Off);
        assert_eq!(lod(Some(5.0), false), FaceLod::Off, "out of view");
        assert_eq!(
            FaceLod::of(Some(500.0), 0.0, false),
            FaceLod::Full,
            "0: always whole"
        );
        assert!(FaceLod::Reduced.interval() < FaceLod::Static.interval());

        let full = FaceDetail::at(FaceLod::Full, true);
        assert!(full.procedural && full.idle && full.dialogue);
        let reduced = FaceDetail::at(FaceLod::Reduced, true);
        assert!(reduced.procedural && !reduced.idle && reduced.dialogue);
        let still = FaceDetail::at(FaceLod::Static, true);
        assert!(!still.procedural && !still.dialogue);
        let switched_off = FaceDetail::at(FaceLod::Full, false);
        assert!(!switched_off.procedural && !switched_off.idle && switched_off.dialogue);
    }

    fn play(name: &str, priority: i32, order: u64) -> ExpressionPlay {
        ExpressionPlay {
            name: name.into(),
            intensity: 1.0,
            target_intensity: 1.0,
            presence: 0.0,
            target_presence: 1.0,
            transition: Some(0.5),
            weight: 1.0,
            priority,
            asymmetry: 0.0,
            order,
        }
    }

    fn at(layer: &LayerValues, name: &str) -> (f32, f32) {
        let control = face_control(name).unwrap();
        (layer.value[control], layer.cover[control])
    }

    #[test]
    fn every_built_in_expression_reads_and_names_only_controls() {
        assert_eq!(built_ins().len(), BUILT_IN_EXPRESSIONS.len());
        for name in BUILT_IN_EXPRESSIONS {
            let expression = built_in_expression(name).unwrap();
            assert!(!expression.controls.is_empty(), "{name}");
            for (control, value) in &expression.controls {
                assert!(face_control(control).is_some(), "{name}: {control}");
                assert!((0.0..=1.0).contains(value), "{name}: {control} {value}");
            }
        }
        assert!(built_in_expression("happy").is_some());
        assert_eq!(expression_names(&FaceExpressions::default()).len(), 15);
    }

    #[test]
    fn expressions_fade_in_blend_by_priority_and_fade_out() {
        let mut state = FaceState::default();
        state.expressions.push(play("Happy", 0, 0));
        let project = FaceExpressions::default();
        let mut unknown = Vec::new();
        let layer = advance_expressions(&mut state, &project, 0.25, &mut unknown);
        let (value, cover) = at(&layer, "Mouth_Smile_Left");
        assert!((cover - 0.5).abs() < 1e-6, "halfway in, eased: {cover}");
        assert!((value - 0.8 * 0.5).abs() < 1e-6);
        let layer = advance_expressions(&mut state, &project, 0.5, &mut unknown);
        assert_eq!(at(&layer, "Mouth_Smile_Left"), (0.8, 1.0));
        assert_eq!(
            at(&layer, "Blink_Left"),
            (0.0, 0.0),
            "Happy leaves blinks alone"
        );

        // Sad at a higher priority goes over Happy where both say something.
        let mut sad = play("Sad", 1, 1);
        sad.presence = 1.0;
        state.expressions.push(sad);
        let layer = advance_expressions(&mut state, &project, 0.0, &mut unknown);
        assert_eq!(at(&layer, "Mouth_Frown_Left"), (0.6, 1.0));
        assert_eq!(
            at(&layer, "Mouth_Smile_Left"),
            (0.8, 1.0),
            "Sad doesn't say"
        );

        // Cleared, Happy fades out and is dropped.
        state.expressions[0].target_presence = 0.0;
        advance_expressions(&mut state, &project, 1.0, &mut unknown);
        assert_eq!(state.expressions.len(), 1);
        assert_eq!(state.expressions[0].name, "Sad");

        state.expressions.push(play("Smug", 0, 2));
        advance_expressions(&mut state, &project, 0.1, &mut unknown);
        assert_eq!(unknown, ["Smug"]);
        assert_eq!(state.expressions.len(), 1);
    }

    #[test]
    fn asymmetry_keeps_one_side() {
        let mut state = FaceState::default();
        let mut happy = play("Happy", 0, 0);
        happy.presence = 1.0;
        happy.asymmetry = -1.0;
        state.expressions.push(happy);
        let layer = advance_expressions(
            &mut state,
            &FaceExpressions::default(),
            0.0,
            &mut Vec::new(),
        );
        assert_eq!(at(&layer, "Mouth_Smile_Left"), (0.8, 1.0));
        assert_eq!(at(&layer, "Mouth_Smile_Right"), (0.0, 0.0));
        assert_eq!(
            at(&layer, "Mouth_Open"),
            (0.15, 1.0),
            "middle controls stay"
        );
    }

    #[test]
    fn blinks_close_and_open_within_their_duration_and_wait_between() {
        let settings = BlinkSettings {
            enabled: true,
            eyes: [true, false],
            interval: (1.0, 1.0),
            duration: 0.2,
            double_chance: 0.0,
        };
        let mut motion = FaceMotion::new(7);
        let mut closed_at = Vec::new();
        let step = 1.0 / 100.0;
        for frame in 0..600 {
            let [left, right] = motion.blink(&settings, step);
            assert_eq!(right, 0.0, "only the left eye");
            if left > 0.99 {
                closed_at.push(frame as f64 * step);
            }
        }
        // Blinks start a second after the last one ended.
        let starts: Vec<f64> = closed_at
            .windows(2)
            .filter(|pair| pair[1] - pair[0] > 0.5)
            .map(|pair| pair[1] - pair[0])
            .collect();
        assert!(!starts.is_empty());
        for gap in starts {
            assert!((gap - 1.2).abs() < 0.05, "{gap}");
        }
        let off = BlinkSettings {
            enabled: false,
            ..settings
        };
        assert_eq!(motion.blink(&off, 0.5), [0.0, 0.0]);
    }

    #[test]
    fn gaze_follows_with_lag_and_fades_in() {
        let mut motion = FaceMotion::new(1);
        let target = Vec3::new(1.0, 0.0, 5.0);
        let (point, amount) = motion.gaze(Some(target), 1.0, 0.1, 0.125).unwrap();
        assert_eq!(point, target, "starts on its first target");
        assert!((amount - 0.5).abs() < 1e-6);
        let moved = Vec3::new(-1.0, 0.0, 5.0);
        let (point, amount) = motion.gaze(Some(moved), 1.0, 0.1, 0.125).unwrap();
        assert!(point.x < 0.0 && point.x > -1.0, "catching up: {point}");
        assert_eq!(amount, 1.0);
        assert!(motion.gaze(None, 1.0, 0.1, 0.1).is_some(), "fading out");
        assert!(motion.gaze(None, 1.0, 0.1, 1.0).is_none());
    }
}
