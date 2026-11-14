//! Retarget profiles (AP2): how a take plays on another skeleton, kept in
//! a `*.retarget.json` file a clip's link names, so many clips share one.
//!
//! A profile is about roles, not bones, so a clip with one still plays on
//! any humanoid:
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "match_rest_pose": true,
//!   "hips": "Scaled",
//!   "plant_feet": true,
//!   "match_hands": false,
//!   "fingers": 1.0,
//!   "roles": { "Head": { "weight": 0.5 }, "LeftHand": { "offset": [0, 0, 10] } }
//! }
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::roles::HumanBone;

/// What a retarget profile's file name ends with.
pub const RETARGET_EXTENSION: &str = ".retarget.json";

const FORMAT_VERSION: u32 = 1;

/// How the hips travel on the model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HipsTravel {
    /// As far as the take's, scaled by how much longer the model's legs
    /// are, so steps and travel agree.
    #[default]
    Scaled,
    /// Exactly as far as the take's (in the take's units).
    Unscaled,
    /// Not across the ground: up and down only, so a walk plays in place.
    InPlace,
}

impl HipsTravel {
    pub const ALL: [HipsTravel; 3] = [
        HipsTravel::Scaled,
        HipsTravel::Unscaled,
        HipsTravel::InPlace,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HipsTravel::Scaled => "Scaled to the legs",
            HipsTravel::Unscaled => "As in the take",
            HipsTravel::InPlace => "In place",
        }
    }
}

/// One role's own settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSetting {
    /// How much of the take's turn the bone gets: 0 keeps the pose it had
    /// (what other layers gave it), 1 all of it.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub weight: f32,
    /// An extra turn about the bone's own axes, in degrees (X, then Y, then
    /// Z), after the take's: fixes a wrist or a head that's a little off.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: [f32; 3],
}

impl Default for RoleSetting {
    fn default() -> Self {
        Self {
            weight: 1.0,
            offset: [0.0; 3],
        }
    }
}

impl RoleSetting {
    /// Whether it changes nothing.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A retarget profile file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetargetProfile {
    pub format_version: u32,
    /// Turns each of the model's bones so it rests the way the take's
    /// skeleton rests before playing the take's motion on it, so a take
    /// made in a T-pose plays right on a model that rests in an A-pose.
    #[serde(default = "yes")]
    pub match_rest_pose: bool,
    #[serde(default)]
    pub hips: HipsTravel,
    /// Puts each foot where the take's is (moved by as much as the hips
    /// are, scaled) with leg IK, so feet on the ground stay there when the
    /// legs are a different shape.
    #[serde(default)]
    pub plant_feet: bool,
    /// Puts each hand where the take's is, from the hips and scaled like
    /// them, with arm IK, so hands that touch something (each other, the
    /// hips, a two-handed weapon) keep touching.
    #[serde(default)]
    pub match_hands: bool,
    /// How much of the take's finger motion plays: 0 holds the fingers
    /// still, 1 plays all of it.
    #[serde(default = "one")]
    pub fingers: f32,
    /// Settings for single roles, by role name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, RoleSetting>,
}

impl Default for RetargetProfile {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            match_rest_pose: true,
            hips: HipsTravel::Scaled,
            plant_feet: false,
            match_hands: false,
            fingers: 1.0,
            roles: BTreeMap::new(),
        }
    }
}

impl RetargetProfile {
    /// Reads a profile, saying what's wrong with one that can't be.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let profile: RetargetProfile = serde_json::from_str(text).map_err(|err| err.to_string())?;
        if profile.format_version != FORMAT_VERSION {
            return Err(format!(
                "it is in retarget profile format {}, and only {FORMAT_VERSION} can be read",
                profile.format_version
            ));
        }
        if let Some(role) = profile
            .roles
            .keys()
            .find(|role| HumanBone::from_name(role).is_none())
        {
            return Err(format!("there's no role named `{role}`"));
        }
        Ok(profile)
    }

    /// Pretty JSON, the same every time for the same profile.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("plain data");
        text.push('\n');
        text
    }

    /// `role`'s settings (the defaults if it has none), with the finger
    /// strength folded into a finger's weight.
    pub fn role(&self, role: HumanBone) -> RoleSetting {
        let mut setting = self
            .roles
            .iter()
            .find(|(name, _)| HumanBone::from_name(name) == Some(role))
            .map(|(_, setting)| *setting)
            .unwrap_or_default();
        if role.is_finger() {
            setting.weight *= self.fingers.clamp(0.0, 1.0);
        }
        setting.weight = setting.weight.clamp(0.0, 1.0);
        setting
    }

    /// Sets `role`'s settings, leaving it out when they're the defaults.
    pub fn set_role(&mut self, role: HumanBone, setting: RoleSetting) {
        self.roles
            .retain(|name, _| HumanBone::from_name(name) != Some(role));
        if !setting.is_default() {
            self.roles.insert(role.name().to_owned(), setting);
        }
    }
}

/// Whether `path` is a retarget profile's.
pub fn is_retarget_profile(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(RETARGET_EXTENSION)
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

fn is_one(value: &f32) -> bool {
    *value == 1.0
}

fn is_zero(value: &[f32; 3]) -> bool {
    *value == [0.0; 3]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_round_trips_and_fills_in_what_it_leaves_out() {
        let mut profile = RetargetProfile {
            plant_feet: true,
            hips: HipsTravel::InPlace,
            fingers: 0.5,
            ..RetargetProfile::default()
        };
        profile.set_role(
            HumanBone::Head,
            RoleSetting {
                weight: 0.25,
                offset: [0.0, 10.0, 0.0],
            },
        );
        let text = profile.to_json();
        assert_eq!(RetargetProfile::from_json(&text).unwrap(), profile);
        assert_eq!(RetargetProfile::from_json(&text).unwrap().to_json(), text);
        // The smallest file is the defaults.
        assert_eq!(
            RetargetProfile::from_json(r#"{ "format_version": 1 }"#).unwrap(),
            RetargetProfile::default()
        );
        // Setting a role back to the defaults leaves it out.
        profile.set_role(HumanBone::Head, RoleSetting::default());
        assert!(profile.roles.is_empty());
    }

    #[test]
    fn bad_profiles_say_why() {
        let unknown = RetargetProfile::from_json(
            r#"{ "format_version": 1, "roles": { "Tail": { "weight": 0 } } }"#,
        )
        .unwrap_err();
        assert!(unknown.contains("`Tail`"), "{unknown}");
        assert!(RetargetProfile::from_json(r#"{ "format_version": 1, "feet": true }"#).is_err());
        let newer = RetargetProfile::from_json(r#"{ "format_version": 3 }"#).unwrap_err();
        assert!(newer.contains("format 3"), "{newer}");
    }

    #[test]
    fn finger_strength_scales_each_fingers_weight() {
        let mut profile = RetargetProfile {
            fingers: 0.5,
            ..RetargetProfile::default()
        };
        let finger = HumanBone::from_name("LeftIndexProximal").unwrap();
        profile.set_role(
            finger,
            RoleSetting {
                weight: 0.5,
                ..RoleSetting::default()
            },
        );
        assert_eq!(profile.role(finger).weight, 0.25);
        assert_eq!(profile.role(HumanBone::Head).weight, 1.0);
        let thumb = HumanBone::from_name("RightThumbDistal").unwrap();
        assert_eq!(profile.role(thumb).weight, 0.5);
    }
}
