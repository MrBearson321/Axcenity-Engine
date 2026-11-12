//! A game's own branding (cohesion pass CW8.1): its icon and banner, named
//! in the manifest's `branding` and checked here. The player uses the icon
//! for its window (falling back to Axcenity's), Build Game writes it into
//! the build (`.ico`, `.icns`, a PNG for Linux), and scripts and ImageLabels
//! reach either picture as `branding://icon` and `branding://banner`.
//!
//! Only the files' headers are read here (sizes and formats), so the
//! project crate needs no image decoder.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::SaveError;

/// The content path prefix that names a branding picture.
pub const SCHEME: &str = "branding://";
/// The smallest and largest icon, in pixels a side.
pub const ICON_SIZES: std::ops::RangeInclusive<u32> = 16..=1024;
/// The smallest banner, in pixels.
pub const BANNER_MIN: (u32, u32) = (640, 160);
/// How wide a banner may be for its height: from 2:1 to 6:1.
pub const BANNER_RATIO: std::ops::RangeInclusive<f32> = 2.0..=6.0;

/// The manifest's `branding`: project-relative paths, such as
/// `assets/branding/icon.png`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branding {
    /// A square PNG: the game's window, taskbar and file icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// A wide PNG or JPEG for menus and loading screens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
}

impl Branding {
    /// Whether neither picture is set.
    pub fn is_empty(&self) -> bool {
        self.icon.is_none() && self.banner.is_none()
    }

    /// The file `reference` names, if it's `branding://icon` or
    /// `branding://banner` and that picture is set.
    pub fn resolve(&self, reference: &str) -> Option<&str> {
        match reference.strip_prefix(SCHEME)? {
            "icon" => self.icon.as_deref(),
            "banner" => self.banner.as_deref(),
            _ => None,
        }
    }
}

/// Which picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picture {
    Icon,
    Banner,
}

impl Picture {
    pub fn name(self) -> &'static str {
        match self {
            Picture::Icon => "icon",
            Picture::Banner => "banner",
        }
    }

    /// What a good one is, in a sentence.
    pub fn wanted(self) -> String {
        match self {
            Picture::Icon => format!(
                "a square PNG from {}×{0} to {}×{1} pixels (256×256 or more looks sharp everywhere)",
                ICON_SIZES.start(),
                ICON_SIZES.end()
            ),
            Picture::Banner => format!(
                "a PNG or JPEG at least {}×{} pixels, 2 to 6 times as wide as it is high",
                BANNER_MIN.0, BANNER_MIN.1
            ),
        }
    }
}

/// An image file's format and size, read from its header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    pub png: bool,
    pub width: u32,
    pub height: u32,
}

/// The format and size of the PNG or JPEG in `bytes`.
pub fn image_info(bytes: &[u8]) -> Result<ImageInfo, String> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG) {
        // The first chunk is IHDR: width and height, big-endian.
        let header = bytes.get(16..24).ok_or("a cut-off PNG file")?;
        let width = u32::from_be_bytes(header[0..4].try_into().unwrap());
        let height = u32::from_be_bytes(header[4..8].try_into().unwrap());
        return Ok(ImageInfo {
            png: true,
            width,
            height,
        });
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return jpeg_size(bytes).map(|(width, height)| ImageInfo {
            png: false,
            width,
            height,
        });
    }
    Err("not a PNG or JPEG file".to_owned())
}

/// A JPEG's size, from its start-of-frame segment.
fn jpeg_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    let mut at = 2;
    while at + 4 <= bytes.len() {
        if bytes[at] != 0xFF {
            return Err("a damaged JPEG file".to_owned());
        }
        let marker = bytes[at + 1];
        // Fill bytes and markers without a length.
        if marker == 0xFF {
            at += 1;
            continue;
        }
        if matches!(marker, 0x01 | 0xD0..=0xD7) {
            at += 2;
            continue;
        }
        let length = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        // SOF0–SOF15, except DHT (C4), JPG (C8) and DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let frame = bytes.get(at + 5..at + 9).ok_or("a cut-off JPEG file")?;
            let height = u16::from_be_bytes([frame[0], frame[1]]) as u32;
            let width = u16::from_be_bytes([frame[2], frame[3]]) as u32;
            return Ok((width, height));
        }
        at += 2 + length;
    }
    Err("a JPEG file with no size".to_owned())
}

/// What's wrong with `picture` at `path` in the project at `folder`, or
/// `None` when it's good.
pub fn check_picture(folder: &Path, picture: Picture, path: &str) -> Option<String> {
    let problem = |why: String| Some(format!("The {} `{path}` {why}", picture.name()));
    if !crate::versions::safe_relative(path) {
        return problem("isn't a path inside the project.".to_owned());
    }
    let bytes = match std::fs::read(folder.join(path)) {
        Ok(bytes) => bytes,
        Err(error) => return problem(format!("can't be read ({error}).")),
    };
    let info = match image_info(&bytes) {
        Ok(info) => info,
        Err(why) => return problem(format!("is {why}; it should be {}.", picture.wanted())),
    };
    let (width, height) = (info.width, info.height);
    let wrong = match picture {
        Picture::Icon => !info.png || width != height || !ICON_SIZES.contains(&width),
        Picture::Banner => {
            let ratio = width as f32 / height.max(1) as f32;
            width < BANNER_MIN.0 || height < BANNER_MIN.1 || !BANNER_RATIO.contains(&ratio)
        }
    };
    if wrong {
        let format = if info.png { "PNG" } else { "JPEG" };
        return problem(format!(
            "is a {width}×{height} {format}; it should be {}.",
            picture.wanted()
        ));
    }
    None
}

/// Every problem with the project's branding (none when it's good or unset).
pub fn check(folder: &Path, branding: &Branding) -> Vec<String> {
    [
        (Picture::Icon, branding.icon.as_deref()),
        (Picture::Banner, branding.banner.as_deref()),
    ]
    .into_iter()
    .filter_map(|(picture, path)| check_picture(folder, picture, path?))
    .collect()
}

/// The project's icon file, when it's set and good: what the player and
/// Build Game use. A bad one falls back to Axcenity's.
pub fn usable_icon(folder: &Path, branding: &Branding) -> Option<std::path::PathBuf> {
    let path = branding.icon.as_deref()?;
    check_picture(folder, Picture::Icon, path)
        .is_none()
        .then(|| folder.join(path))
}

/// Stores the project's branding in its manifest.
pub fn set_branding(folder: &Path, branding: &Branding) -> Result<(), SaveError> {
    let value = serde_json::to_value(branding).expect("branding is plain data");
    crate::update_manifest(folder, "branding", value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }

    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8];
        // An APP0 segment first, then the frame.
        bytes.extend([0xFF, 0xE0, 0, 4, 0, 0]);
        bytes.extend([0xFF, 0xC0, 0, 11, 8]);
        bytes.extend(height.to_be_bytes());
        bytes.extend(width.to_be_bytes());
        bytes.extend([3, 1, 0x11, 0]);
        bytes
    }

    #[test]
    fn png_and_jpeg_sizes_are_read_from_their_headers() {
        assert_eq!(
            image_info(&png(256, 128)),
            Ok(ImageInfo {
                png: true,
                width: 256,
                height: 128
            })
        );
        assert_eq!(
            image_info(&jpeg(1280, 320)),
            Ok(ImageInfo {
                png: false,
                width: 1280,
                height: 320
            })
        );
        assert!(image_info(b"GIF89a").is_err());
        assert!(image_info(&png(1, 1)[..12]).is_err());
    }

    #[test]
    fn icons_and_banners_are_checked_for_format_size_and_shape() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-branding-test-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("assets")).unwrap();
        for (name, bytes) in [
            ("icon.png", png(256, 256)),
            ("tall.png", png(256, 300)),
            ("huge.png", png(2048, 2048)),
            ("icon.jpg", jpeg(256, 256)),
            ("banner.jpg", jpeg(1280, 320)),
            ("square.png", png(800, 800)),
            ("small.png", png(320, 80)),
        ] {
            std::fs::write(folder.join("assets").join(name), bytes).unwrap();
        }
        let icon = |path: &str| check_picture(&folder, Picture::Icon, path);
        let banner = |path: &str| check_picture(&folder, Picture::Banner, path);
        assert_eq!(icon("assets/icon.png"), None);
        assert!(icon("assets/tall.png").unwrap().contains("256×300 PNG"));
        assert!(icon("assets/huge.png").is_some());
        assert!(icon("assets/icon.jpg").unwrap().contains("JPEG"));
        assert!(
            icon("assets/missing.png")
                .unwrap()
                .contains("can't be read")
        );
        assert!(
            icon("../outside.png")
                .unwrap()
                .contains("inside the project")
        );
        assert_eq!(banner("assets/banner.jpg"), None);
        assert!(banner("assets/square.png").is_some());
        assert!(banner("assets/small.png").is_some());

        let good = Branding {
            icon: Some("assets/icon.png".into()),
            banner: Some("assets/banner.jpg".into()),
        };
        assert!(check(&folder, &good).is_empty());
        assert_eq!(
            usable_icon(&folder, &good),
            Some(folder.join("assets/icon.png"))
        );
        let bad = Branding {
            icon: Some("assets/tall.png".into()),
            banner: Some("assets/small.png".into()),
        };
        assert_eq!(check(&folder, &bad).len(), 2);
        assert_eq!(usable_icon(&folder, &bad), None);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn branding_paths_resolve_and_unset_branding_is_left_out_of_the_manifest() {
        let branding = Branding {
            icon: None,
            banner: Some("assets/banner.png".into()),
        };
        assert_eq!(
            branding.resolve("branding://banner"),
            Some("assets/banner.png")
        );
        assert_eq!(branding.resolve("branding://icon"), None);
        assert_eq!(branding.resolve("assets/banner.png"), None);
        assert_eq!(
            serde_json::to_string(&branding).unwrap(),
            r#"{"banner":"assets/banner.png"}"#
        );
        assert!(Branding::default().is_empty());
    }
}
