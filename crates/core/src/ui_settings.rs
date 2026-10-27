//! The player's game UI settings: text scale, higher contrast and reduced
//! motion. They change how HUDs and menus are drawn and moved, never the 3D
//! world, and they're the game's, not the editor's appearance settings.
//!
//! The [`World`](crate::World) keeps them ([`World::ui_settings`](crate::World::ui_settings)).
//! A save slot keeps them as settings named [`UI_SETTING_PREFIX`] and the
//! setting (`UI/TextScale`), beside the game's own settings, so they come
//! back when it loads.

use crate::Value;

/// The settings name each game UI setting is kept under, before its name
/// (`UI/TextScale`).
pub const UI_SETTING_PREFIX: &str = "UI/";

/// The smallest and largest text scale.
pub const TEXT_SCALE_RANGE: (f64, f64) = (0.5, 3.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSettings {
    /// How much bigger UI text is drawn than it's authored; 1 is as authored.
    /// Text that fits its box (`TextScaled`) doesn't change.
    pub text_scale: f64,
    /// Text is drawn black or white, whichever is nearer its colour, and
    /// see-through backgrounds are drawn solid.
    pub high_contrast: bool,
    /// UI motion that's only for looks happens at once: CSS transitions,
    /// tweens of UI elements, and their Animations and AnimationTracks.
    pub reduced_motion: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            text_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
        }
    }
}

/// The names of the settings, after [`UI_SETTING_PREFIX`].
pub const UI_SETTING_NAMES: [&str; 3] = ["TextScale", "HighContrast", "ReducedMotion"];

impl UiSettings {
    /// A text scale kept in range, or `None` when `scale` isn't a number
    /// in [`TEXT_SCALE_RANGE`].
    pub fn checked_text_scale(scale: f64) -> Option<f64> {
        let (min, max) = TEXT_SCALE_RANGE;
        (min..=max).contains(&scale).then_some(scale)
    }

    /// Each setting as it's kept in a save slot: its full name and value.
    pub fn to_settings(&self) -> [(String, Value); 3] {
        let [scale, contrast, motion] = UI_SETTING_NAMES;
        [
            (
                format!("{UI_SETTING_PREFIX}{scale}"),
                Value::Number(self.text_scale),
            ),
            (
                format!("{UI_SETTING_PREFIX}{contrast}"),
                Value::Bool(self.high_contrast),
            ),
            (
                format!("{UI_SETTING_PREFIX}{motion}"),
                Value::Bool(self.reduced_motion),
            ),
        ]
    }

    /// The value of the setting `name` (with its prefix), or `None` when it
    /// isn't one of these.
    pub fn get(&self, name: &str) -> Option<Value> {
        self.to_settings()
            .into_iter()
            .find(|(full, _)| full == name)
            .map(|(_, value)| value)
    }

    /// Sets the setting `name` (with its prefix) from a saved `value`.
    /// `Ok(false)` when `name` isn't one of these; an error saying what it
    /// takes when `value` doesn't fit, and nothing changes.
    pub fn set(&mut self, name: &str, value: &Value) -> Result<bool, String> {
        let Some(setting) = name.strip_prefix(UI_SETTING_PREFIX) else {
            return Ok(false);
        };
        match (setting, value) {
            ("TextScale", Value::Number(scale)) => {
                let (min, max) = TEXT_SCALE_RANGE;
                self.text_scale = Self::checked_text_scale(*scale)
                    .ok_or_else(|| format!("{name} must be a number from {min} to {max}"))?;
            }
            ("HighContrast", Value::Bool(on)) => self.high_contrast = *on,
            ("ReducedMotion", Value::Bool(on)) => self.reduced_motion = *on,
            ("TextScale", _) => return Err(format!("{name} must be a number")),
            ("HighContrast" | "ReducedMotion", _) => {
                return Err(format!("{name} must be true or false"));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// These settings with the ones `settings` has applied: the settings a
    /// save slot loaded. A missing one or one that doesn't fit stays as it
    /// is, so a slot from before these settings changes nothing.
    pub fn with_settings<'a>(
        &self,
        settings: impl IntoIterator<Item = (&'a str, &'a Value)>,
    ) -> Self {
        let mut applied = *self;
        for (name, value) in settings {
            let mut one = applied;
            if one.set(name, value) == Ok(true) {
                applied = one;
            }
        }
        applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_their_saved_names() {
        let settings = UiSettings {
            text_scale: 1.5,
            high_contrast: true,
            reduced_motion: true,
        };
        let saved = settings.to_settings();
        assert_eq!(saved[0].0, "UI/TextScale");
        let restored = UiSettings::default()
            .with_settings(saved.iter().map(|(name, value)| (name.as_str(), value)));
        assert_eq!(restored, settings);
        assert_eq!(settings.get("UI/HighContrast"), Some(Value::Bool(true)));
        assert_eq!(settings.get("Volume"), None);
    }

    #[test]
    fn a_wrong_or_missing_value_changes_nothing() {
        let mut settings = UiSettings::default();
        assert_eq!(settings.set("Volume", &Value::Number(3.0)), Ok(false));
        assert!(settings.set("UI/TextScale", &Value::Number(9.0)).is_err());
        assert!(
            settings
                .set("UI/ReducedMotion", &Value::Number(1.0))
                .is_err()
        );
        assert_eq!(settings, UiSettings::default());
        let loaded = settings.with_settings([("UI/TextScale", &Value::Bool(true))]);
        assert_eq!(loaded, UiSettings::default());
    }
}
