//! Reflection probe bakes: what a `ReflectionProbe` saw from the middle of
//! its part, baked in the editor and named by its `Bake` property.
//!
//! A bake is a Radiance `.hdr` file (so light brighter than 1 survives and
//! any HDR viewer opens it) holding the six faces of a cube, square and
//! stacked top to bottom in the order GPUs keep them: +X, -X, +Y, -Y, +Z,
//! -Z. Each face is as a cubemap stores it, looking out from the middle.

use std::path::Path;
use std::sync::Arc;

use crate::model::new_key;
use crate::safe_path;

/// Where the editor saves bakes.
pub const PROBES_FOLDER: &str = "assets/probes";
/// The end of every bake's file name.
pub const PROBE_SUFFIX: &str = ".probe.hdr";

/// Whether `path` names a reflection probe bake, by its name.
pub fn is_probe_file(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(PROBE_SUFFIX)
}

/// The project path a bake named `name` is saved at, such as
/// `assets/probes/Hall.probe.hdr`.
pub fn probe_path(name: &str) -> String {
    format!("{PROBES_FOLDER}/{name}{PROBE_SUFFIX}")
}

/// A reflection probe's bake in linear light, as the renderer reads it.
#[derive(Clone)]
pub struct ProbeImage {
    key: u64,
    /// Each face's width and height in pixels.
    pub size: u32,
    /// Red, green and blue for each pixel of the six faces, one after the
    /// other in cubemap order, each face's rows from the top. Shared, so
    /// handing it to the renderer each frame copies nothing.
    pub rgb: Arc<Vec<f32>>,
}

impl ProbeImage {
    /// A bake of six `size` × `size` faces (see [`ProbeImage::rgb`]), or
    /// `None` if `rgb` isn't that long.
    pub fn new(size: u32, rgb: Vec<f32>) -> Option<Self> {
        (size > 0 && rgb.len() == (size * size * 6 * 3) as usize).then(|| Self {
            key: new_key(),
            size,
            rgb: Arc::new(rgb),
        })
    }

    /// Different for every bake read or made.
    pub fn key(&self) -> u64 {
        self.key
    }
}

impl std::fmt::Debug for ProbeImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProbeImage #{} ({}² × 6)", self.key, self.size)
    }
}

/// Reads the bake at `path` inside `folder`, or explains why it can't. Not
/// cached: [`Assets::reflection_probe`](crate::Assets::reflection_probe) is
/// the cached way.
pub fn read_probe(folder: &Path, path: &str) -> Result<ProbeImage, String> {
    let relative = safe_path(path).ok_or(
        "a reflection probe's bake must be a file inside the project folder, \
         such as \"assets/probes/Hall.probe.hdr\"",
    )?;
    let bytes = std::fs::read(folder.join(&relative)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })?;
    decode_probe(&bytes)
}

/// Decodes a bake file's `bytes`.
pub fn decode_probe(bytes: &[u8]) -> Result<ProbeImage, String> {
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Hdr)
        .map_err(|err| format!("it isn't a Radiance .hdr file ({err})"))?;
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height != width * 6 {
        return Err(format!(
            "it is {width}×{height}, not six square faces stacked top to bottom"
        ));
    }
    ProbeImage::new(width, decoded.into_rgb32f().into_raw())
        .ok_or_else(|| "it has the wrong number of pixels".to_owned())
}

/// A bake of six `size` × `size` faces (`rgb` as in [`ProbeImage::rgb`]) as
/// a Radiance `.hdr` file's bytes.
pub fn encode_probe(size: u32, rgb: &[f32]) -> Result<Vec<u8>, String> {
    if size == 0 || rgb.len() != (size * size * 6 * 3) as usize {
        return Err("a bake needs six square faces".to_owned());
    }
    let pixels: Vec<image::Rgb<f32>> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        // Radiance files hold no negative or missing light.
        .map(|&color| image::Rgb(color.map(|c| if c.is_finite() { c.max(0.0) } else { 0.0 })))
        .collect();
    let mut bytes = Vec::new();
    image::codecs::hdr::HdrEncoder::new(&mut bytes)
        .encode(&pixels, size as usize, size as usize * 6)
        .map_err(|err| err.to_string())?;
    Ok(bytes)
}

/// Saves a bake at `path` inside `folder`, making its folder if needed.
pub fn write_probe(folder: &Path, path: &str, size: u32, rgb: &[f32]) -> Result<(), String> {
    let relative = safe_path(path).ok_or("a bake must be saved inside the project folder")?;
    let bytes = encode_probe(size, rgb)?;
    let file = folder.join(relative);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(file, bytes).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six faces of `size`, each a color of its own: face `i` is `(i, 1, 2)`.
    fn faces(size: u32) -> Vec<f32> {
        (0..6)
            .flat_map(|face| [face as f32, 1.0, 2.0].repeat((size * size) as usize))
            .collect()
    }

    #[test]
    fn a_bake_round_trips_through_its_file() {
        let folder = std::env::temp_dir().join(format!("axcenity-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = probe_path("Hall");
        assert_eq!(path, "assets/probes/Hall.probe.hdr");
        assert!(is_probe_file(&path) && !is_probe_file("assets/sky/noon.hdr"));
        write_probe(&folder, &path, 4, &faces(4)).unwrap();
        let read = read_probe(&folder, &path).unwrap();
        assert_eq!(read.size, 4);
        for face in 0..6 {
            let at = (face * 4 * 4 * 3) as usize;
            let color = &read.rgb[at..at + 3];
            // Radiance keeps 8 bits of each color beside a shared exponent.
            for (got, wrote) in color.iter().zip([face as f32, 1.0, 2.0]) {
                assert!(
                    (got - wrote).abs() <= 0.02 * wrote.max(1.0),
                    "{face}: {color:?}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_missing_or_misshapen_bake_says_why() {
        let folder = std::env::temp_dir();
        assert_eq!(
            read_probe(&folder, "assets/probes/nowhere.probe.hdr").unwrap_err(),
            "the file isn't there"
        );
        let mut bytes = Vec::new();
        image::codecs::hdr::HdrEncoder::new(&mut bytes)
            .encode(&[image::Rgb([1.0, 1.0, 1.0]); 8], 4, 2)
            .unwrap();
        assert!(
            decode_probe(&bytes)
                .unwrap_err()
                .contains("six square faces")
        );
        assert!(encode_probe(4, &[0.0; 3]).is_err());
    }
}
