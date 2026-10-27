//! Sky images: high dynamic range pictures of the whole sky that Lighting's
//! `SkyImage` names, wrapped around the world (equirectangular).
//!
//! Radiance `.hdr` files hold light brighter than 1, as a real sky has, so
//! the sun in them lights and blooms. A PNG or JPEG can stand in, its
//! colors read as sRGB, though nothing in it is brighter than white.

use std::path::Path;
use std::sync::Arc;

use crate::model::new_key;
use crate::safe_path;

/// The file extensions a sky image can have.
pub const SKY_IMAGE_EXTENSIONS: [&str; 4] = ["hdr", "png", "jpg", "jpeg"];

/// A sky image in linear light, as the renderer draws it.
#[derive(Clone)]
pub struct SkyImage {
    key: u64,
    pub width: u32,
    pub height: u32,
    /// Red, green and blue for each pixel, rows from the top. Shared, so
    /// handing it to the renderer each frame copies nothing.
    pub rgb: Arc<Vec<f32>>,
}

impl SkyImage {
    /// Different for every sky image read.
    pub fn key(&self) -> u64 {
        self.key
    }
}

impl std::fmt::Debug for SkyImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SkyImage #{} ({}×{})", self.key, self.width, self.height)
    }
}

/// Reads the sky image at `path` inside `folder`, or explains why it
/// can't. Not cached: [`Assets::sky_image`](crate::Assets::sky_image) is
/// the cached way.
pub fn read_sky_image(folder: &Path, path: &str) -> Result<SkyImage, String> {
    let relative = safe_path(path).ok_or(
        "a sky image must be a file inside the project folder, such as \"assets/sky/noon.hdr\"",
    )?;
    let bytes = std::fs::read(folder.join(&relative)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })?;
    decode_sky_image(&bytes, path)
}

/// Decodes a sky image file's `bytes`; `name` gives its type when the bytes
/// don't.
pub fn decode_sky_image(bytes: &[u8], name: &str) -> Result<SkyImage, String> {
    const ONLY: &str = "only Radiance .hdr, PNG and JPEG sky images can be used";
    let format = image::guess_format(bytes)
        .or_else(|_| image::ImageFormat::from_path(name))
        .map_err(|_| ONLY.to_owned())?;
    if !matches!(
        format,
        image::ImageFormat::Hdr | image::ImageFormat::Png | image::ImageFormat::Jpeg
    ) {
        return Err(ONLY.to_owned());
    }
    let decoded = image::load_from_memory_with_format(bytes, format).map_err(|err| match err {
        image::ImageError::Unsupported(_) => ONLY.to_owned(),
        err => err.to_string(),
    })?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err("it has no pixels".to_owned());
    }
    let mut rgb = decoded.to_rgb32f();
    // An .hdr file holds linear light already; other images hold sRGB.
    if format != image::ImageFormat::Hdr {
        for channel in rgb.iter_mut() {
            *channel = srgb_to_linear(*channel);
        }
    }
    Ok(SkyImage {
        key: new_key(),
        width: rgb.width(),
        height: rgb.height(),
        rgb: Arc::new(rgb.into_raw()),
    })
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width` × `height` Radiance file of one color.
    pub(crate) fn hdr_file(width: u32, height: u32, rgb: [f32; 3]) -> Vec<u8> {
        let pixels = vec![image::Rgb(rgb); (width * height) as usize];
        let mut bytes = Vec::new();
        image::codecs::hdr::HdrEncoder::new(&mut bytes)
            .encode(&pixels, width as usize, height as usize)
            .unwrap();
        bytes
    }

    #[test]
    fn an_hdr_file_keeps_light_brighter_than_white() {
        let image = decode_sky_image(&hdr_file(4, 2, [6.0, 1.0, 0.25]), "sky.hdr").unwrap();
        assert_eq!((image.width, image.height), (4, 2));
        assert_eq!(image.rgb.len(), 4 * 2 * 3);
        // Radiance files keep 8 bits of each color beside a shared exponent.
        let first = &image.rgb[..3];
        for (read, wrote) in first.iter().zip([6.0, 1.0, 0.25]) {
            assert!((read - wrote).abs() < wrote * 0.05, "{first:?}");
        }
    }

    #[test]
    fn a_png_sky_is_read_as_srgb_and_turned_linear() {
        let mut bytes = Vec::new();
        image::RgbImage::from_pixel(2, 1, image::Rgb([255, 128, 0]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let image = decode_sky_image(&bytes, "sky.png").unwrap();
        assert_eq!(image.rgb[0], 1.0);
        assert!(
            (image.rgb[1] - 0.2158).abs() < 0.01,
            "{:?}",
            &image.rgb[..3]
        );
        assert_eq!(image.rgb[2], 0.0);
    }

    #[test]
    fn assets_read_a_sky_image_once_and_report_a_missing_one_once() {
        let folder = std::env::temp_dir().join(format!("axcenity-sky-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/sky")).unwrap();
        std::fs::write(folder.join("assets/sky/noon.hdr"), hdr_file(8, 4, [2.0; 3])).unwrap();
        let assets = crate::Assets::new(&folder);
        let first = assets.sky_image("assets/sky/noon.hdr").unwrap();
        let again = assets.sky_image("assets/sky/noon.hdr").unwrap();
        assert!(Arc::ptr_eq(&first, &again), "read once and kept");
        assert!(assets.take_errors().is_empty());

        assert!(assets.sky_image("assets/sky/gone.hdr").is_none());
        assert!(assets.sky_image("assets/sky/gone.hdr").is_none());
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("Could not load the sky image `assets/sky/gone.hdr`"));
        assert!(assets.sky_image("").is_none());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn other_files_are_refused_with_a_reason() {
        let error = decode_sky_image(b"not an image", "sky.txt").unwrap_err();
        assert!(error.contains("Radiance .hdr"), "{error}");
        let error = read_sky_image(Path::new("."), "../outside.hdr").unwrap_err();
        assert!(error.contains("inside the project folder"), "{error}");
    }
}
