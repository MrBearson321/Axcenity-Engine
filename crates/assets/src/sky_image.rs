//! Sky images: high dynamic range pictures of the whole sky that Lighting's
//! `SkyImage` names, wrapped around the world (equirectangular).
//!
//! OpenEXR (`.exr`) and Radiance (`.hdr`) files hold light brighter than 1,
//! as a real sky has, so the sun in them lights and blooms. A PNG or JPEG
//! can stand in, its colors read as sRGB, though nothing in it is brighter
//! than white.
//!
//! Images wider than [`MAX_SKY_WIDTH`] are scaled down as they're read: more
//! detail than that is lost on screen, and an 8K panorama in linear light
//! takes 400 MB.

use std::path::Path;
use std::sync::Arc;

use crate::model::new_key;
use crate::safe_path;

/// The file extensions a sky image can have.
pub const SKY_IMAGE_EXTENSIONS: [&str; 5] = ["exr", "hdr", "png", "jpg", "jpeg"];
/// The extensions of the high dynamic range files, which are only ever sky
/// images (PNG and JPEG are usually textures).
pub const HDR_SKY_EXTENSIONS: [&str; 2] = ["exr", "hdr"];
/// The widest a sky image is kept, in pixels.
pub const MAX_SKY_WIDTH: u32 = 4096;

/// Whether `path` names a high dynamic range sky image file, by its
/// extension. A reflection probe's bake is an `.hdr` file too, but not a sky.
pub fn is_hdr_sky_file(path: &str) -> bool {
    !crate::is_probe_file(path)
        && path.rsplit_once('.').is_some_and(|(_, extension)| {
            HDR_SKY_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

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

    /// Why it may look wrong, if it isn't a panorama twice as wide as it is
    /// tall (2:1, within 2%): wrapped around the sky, it's stretched.
    pub fn shape_warning(&self) -> Option<String> {
        let ratio = self.width as f32 / self.height as f32;
        ((ratio - 2.0).abs() > 0.04).then(|| {
            format!(
                "is {}×{}, not twice as wide as it is tall, so it's stretched around the sky. \
                 Use a panorama (equirectangular, 2:1), such as an HDRI",
                self.width, self.height
            )
        })
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
    const ONLY: &str = "only OpenEXR, Radiance .hdr, PNG and JPEG sky images can be used";
    use image::ImageFormat;
    let format = image::guess_format(bytes)
        .or_else(|_| ImageFormat::from_path(name))
        .map_err(|_| ONLY.to_owned())?;
    if !matches!(
        format,
        ImageFormat::OpenExr | ImageFormat::Hdr | ImageFormat::Png | ImageFormat::Jpeg
    ) {
        return Err(ONLY.to_owned());
    }
    let mut decoded =
        image::load_from_memory_with_format(bytes, format).map_err(|err| match err {
            image::ImageError::Unsupported(_) => ONLY.to_owned(),
            err => err.to_string(),
        })?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err("it has no pixels".to_owned());
    }
    // Scaled down in its own format first, so a huge file never takes the
    // memory of a huge image in linear light.
    if decoded.width() > MAX_SKY_WIDTH {
        let height = (u64::from(decoded.height()) * u64::from(MAX_SKY_WIDTH)
            / u64::from(decoded.width()))
        .max(1) as u32;
        decoded =
            decoded.resize_exact(MAX_SKY_WIDTH, height, image::imageops::FilterType::Triangle);
    }
    let (width, height) = (decoded.width(), decoded.height());
    let rgb = if matches!(format, ImageFormat::OpenExr | ImageFormat::Hdr) {
        // Linear light already.
        decoded.into_rgb32f().into_raw()
    } else if decoded.color().bytes_per_pixel() / decoded.color().channel_count() == 1 {
        // 8 bits a channel: each of the 256 values turned linear once.
        let linear: [f32; 256] = std::array::from_fn(|byte| srgb_to_linear(byte as f32 / 255.0));
        decoded
            .into_rgb8()
            .into_raw()
            .into_iter()
            .map(|byte| linear[usize::from(byte)])
            .collect()
    } else {
        let mut rgb = decoded.into_rgb32f().into_raw();
        for channel in &mut rgb {
            *channel = srgb_to_linear(*channel);
        }
        rgb
    };
    Ok(SkyImage {
        key: new_key(),
        width,
        height,
        rgb: Arc::new(rgb),
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
    fn an_exr_file_keeps_light_brighter_than_white() {
        let (width, height) = (8u32, 4u32);
        let pixels: Vec<f32> = [12.5, 0.75, 0.125].repeat((width * height) as usize);
        let mut bytes = Vec::new();
        image::Rgb32FImage::from_raw(width, height, pixels)
            .unwrap()
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::OpenExr,
            )
            .unwrap();
        let image = decode_sky_image(&bytes, "sky.exr").unwrap();
        assert_eq!((image.width, image.height), (8, 4));
        assert_eq!(&image.rgb[..3], &[12.5, 0.75, 0.125], "kept exactly");
        assert!(image.shape_warning().is_none(), "2:1 is a panorama");
        assert!(is_hdr_sky_file("assets/sky/Noon.EXR") && is_hdr_sky_file("a/b.hdr"));
        assert!(!is_hdr_sky_file("assets/sky/noon.png"));
        assert!(!is_hdr_sky_file("assets/probes/Hall.probe.hdr"));
    }

    #[test]
    fn a_huge_sky_is_scaled_down_to_the_widest_kept() {
        let image = decode_sky_image(&hdr_file(MAX_SKY_WIDTH * 2, 8, [1.0; 3]), "big.hdr").unwrap();
        assert_eq!((image.width, image.height), (MAX_SKY_WIDTH, 4));
        assert_eq!(image.rgb.len(), (MAX_SKY_WIDTH * 4 * 3) as usize);
    }

    #[test]
    fn a_picture_that_isnt_a_panorama_says_it_will_stretch() {
        let image = decode_sky_image(&hdr_file(16, 9, [1.0; 3]), "photo.hdr").unwrap();
        let warning = image.shape_warning().unwrap();
        assert!(
            warning.contains("16×9") && warning.contains("2:1"),
            "{warning}"
        );
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
    fn in_the_background_a_sky_arrives_later_with_a_wake_up() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-sky-background-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/sky")).unwrap();
        std::fs::write(
            folder.join("assets/sky/wide.hdr"),
            hdr_file(16, 9, [1.0; 3]),
        )
        .unwrap();
        let assets = crate::Assets::new(&folder);
        let (woken, wakes) = std::sync::mpsc::channel();
        assets.load_textures_in_background(move || {
            let _ = woken.send(());
        });
        // Asked for, not read yet: the sky colors show meanwhile.
        assert!(assets.sky_image("assets/sky/wide.hdr").is_none());
        wakes
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the sky wakes the host");
        assert!(assets.texture_progress().is_none());
        assert_eq!(assets.sky_image("assets/sky/wide.hdr").unwrap().width, 16);
        // Not a panorama, so it's said once.
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("stretched"), "{errors:?}");
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
