//! The default sky: a CC0 HDRI from Poly Haven (the repository's
//! `assets/sky/`, credited in its `README.md`), built into the engine so
//! every new blank project starts under a real sky instead of the sky
//! colors. New Project's Baseplate and Empty templates write it into the
//! project and set it as Lighting's `SkyImage`; the editor's sky row puts
//! it into any other project (`docs/rendering/sky.md`).

use super::*;

/// Where the default sky goes in a project, and Lighting's `SkyImage` for it.
pub const DEFAULT_SKY_PATH: &str = "assets/sky/partly-cloudy-sky.hdr";

/// The default sky's file: Radiance `.hdr`, 2048 × 1024.
pub static DEFAULT_SKY: &[u8] = include_bytes!("../../../assets/sky/partly-cloudy-sky.hdr");

/// Lighting's `ClockTime` that, with [`DEFAULT_SKY_LATITUDE`], puts the sun
/// (and so the shadows) where the default sky's sun is: 47.9° up, toward
/// -Z and a little +X (`scene::sun_direction`). The picture was taken south
/// of the equator, so the sun is to the north at mid-morning.
pub const DEFAULT_SKY_CLOCK_TIME: f64 = 10.52;

/// Lighting's `GeographicLatitude` for the default sky's sun; see
/// [`DEFAULT_SKY_CLOCK_TIME`].
pub const DEFAULT_SKY_LATITUDE: f64 = -36.77;

/// Writes the default sky into `project` at [`DEFAULT_SKY_PATH`] and returns
/// that path for Lighting's `SkyImage`. A file already there is kept, so
/// this never writes over a creator's own.
pub fn add_default_sky(project: &Path) -> Result<&'static str, SaveError> {
    let file = project.join(DEFAULT_SKY_PATH);
    if file.is_file() {
        return Ok(DEFAULT_SKY_PATH);
    }
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder).map_err(SaveError::new(folder))?;
    }
    fs::write(&file, DEFAULT_SKY).map_err(SaveError::new(&file))?;
    Ok(DEFAULT_SKY_PATH)
}
