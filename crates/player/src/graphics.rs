//! The machine's graphics settings for a game: `graphics.json` in the
//! game's player data folder (`axcenity_save::game_folder`), never in the
//! game's own folder.

use std::fs;
use std::path::Path;

use axcenity_render::{Capabilities, GraphicsSettings};

/// The settings file's name in the game's player data folder.
pub const FILE_NAME: &str = "graphics.json";

/// The settings saved in `folder`, or `None` when there's no file or it
/// doesn't read as settings.
pub fn load(folder: &Path) -> Option<GraphicsSettings> {
    let path = folder.join(FILE_NAME);
    let text = fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&text) {
        Ok(settings) => Some(settings),
        Err(err) => {
            log::warn!(
                "{} doesn't read as graphics settings: {err}",
                path.display()
            );
            None
        }
    }
}

/// Writes `settings` to `folder`, making the folder if it's missing.
pub fn save(folder: &Path, settings: &GraphicsSettings) -> std::io::Result<()> {
    fs::create_dir_all(folder)?;
    let text = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    fs::write(folder.join(FILE_NAME), text)
}

/// The settings a game starts with: the saved ones, or else the machine's
/// default (High, or Low on a software adapter), which is then saved so
/// there's a file to change. `folder` is `None` when the player has no data
/// folder, and nothing is saved.
pub fn start(folder: Option<&Path>, capabilities: &Capabilities) -> GraphicsSettings {
    if let Some(settings) = folder.and_then(load) {
        return settings;
    }
    let settings = GraphicsSettings::default_for(capabilities);
    if let Some(folder) = folder
        && let Err(err) = save(folder, &settings)
    {
        log::warn!(
            "could not save the graphics settings in {}: {err}",
            folder.display()
        );
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_render::{Quality, ShadowQuality};

    fn temp_folder(test: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-graphics-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn graphics_json_round_trips_the_settings() {
        let folder = temp_folder("round-trip");
        let settings = GraphicsSettings {
            quality: Quality::Ultra,
            shadow_quality: Some(ShadowQuality::Low),
            shadow_distance: Some(90.0),
            post_processing: Some(false),
        };
        save(&folder, &settings).unwrap();
        assert_eq!(load(&folder), Some(settings));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_first_start_saves_high_or_low_on_a_software_adapter() {
        let folder = temp_folder("first-start");
        let card = Capabilities::default();
        assert_eq!(start(Some(&folder), &card).quality, Quality::High);
        assert_eq!(load(&folder).unwrap().quality, Quality::High);

        // Saved settings win over the machine's default.
        let software = Capabilities {
            software: true,
            ..card
        };
        assert_eq!(start(Some(&folder), &software).quality, Quality::High);
        fs::remove_dir_all(&folder).unwrap();
        assert_eq!(start(Some(&folder), &software).quality, Quality::Low);

        fs::write(folder.join(FILE_NAME), "not json").unwrap();
        assert_eq!(start(Some(&folder), &card).quality, Quality::High);
        assert_eq!(start(None, &software).quality, Quality::Low);
        fs::remove_dir_all(&folder).unwrap();
    }
}
