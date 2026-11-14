//! Finding the texture files a converted model names, and turning them into
//! the images glTF wants: PNG or JPEG files, with metalness and roughness
//! in one image's blue and green, opacity in the color's alpha, and
//! occlusion in red.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use image::{DynamicImage, GenericImageView, ImageFormat, RgbaImage};

use crate::glb::EmbeddedImage;

/// Where a model's texture files are: folders on disk (the root file's,
/// then each of its layers'), and for a `.usdz` the files inside it.
pub(crate) struct Files {
    folders: Vec<PathBuf>,
    /// A `.usdz` archive's files, by their path inside it (lowercase, `/`).
    packaged: HashMap<String, Vec<u8>>,
}

impl Files {
    /// The files for a model whose root file is `root` and whose other
    /// layers are `layers` (paths or identifiers; others are skipped).
    pub(crate) fn new(root: &Path, layers: &[String]) -> Self {
        let mut folders = Vec::new();
        let mut add = |folder: Option<&Path>| {
            if let Some(folder) = folder
                && !folders.iter().any(|known| known == folder)
            {
                folders.push(folder.to_path_buf());
            }
        };
        add(root.parent());
        for layer in layers {
            // `kitchen.usdz[model.usdc]`: inside an archive, already covered.
            let path = layer.split('[').next().unwrap_or(layer);
            add(Path::new(path).parent());
        }
        let packaged = if root
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("usdz"))
        {
            read_archive(root)
        } else {
            HashMap::new()
        };
        Self { folders, packaged }
    }

    /// The file an asset path names, as its file name and bytes, if it's
    /// found. Paths may be `./textures/a.png`, `textures\a.png` or, inside a
    /// `.usdz`, `kitchen.usdz[textures/a.png]`.
    pub(crate) fn read(&self, asset: &str) -> Option<(String, Vec<u8>)> {
        let asset = asset.trim().trim_matches('@');
        let inner = match (asset.find('['), asset.rfind(']')) {
            (Some(start), Some(end)) if start < end => &asset[start + 1..end],
            _ => asset,
        };
        let normal = inner.replace('\\', "/");
        let normal = normal.trim_start_matches("./");
        let name = normal.rsplit('/').next().unwrap_or(normal).to_owned();
        if name.is_empty() {
            return None;
        }
        if !self.packaged.is_empty() {
            let key = normal.to_ascii_lowercase();
            if let Some(bytes) = self.packaged.get(&key) {
                return Some((name, bytes.clone()));
            }
            // By name alone, when only one file in the archive has it.
            let lower = name.to_ascii_lowercase();
            let mut same = self
                .packaged
                .iter()
                .filter(|(path, _)| path.rsplit('/').next() == Some(lower.as_str()));
            if let (Some((_, bytes)), None) = (same.next(), same.next()) {
                return Some((name, bytes.clone()));
            }
        }
        let path = Path::new(normal);
        if path.is_absolute() {
            return std::fs::read(path).ok().map(|bytes| (name, bytes));
        }
        self.folders
            .iter()
            .find_map(|folder| std::fs::read(folder.join(path)).ok())
            .map(|bytes| (name, bytes))
    }
}

/// Every file in a `.usdz` (a zip archive) besides its USD layers.
fn read_archive(path: &Path) -> HashMap<String, Vec<u8>> {
    let mut files = HashMap::new();
    let Ok(bytes) = std::fs::read(path) else {
        return files;
    };
    let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        return files;
    };
    for index in 0..archive.len() {
        let Ok(mut file) = archive.by_index(index) else {
            continue;
        };
        let name = file.name().replace('\\', "/").to_ascii_lowercase();
        if file.is_dir() || crate::is_converted(Path::new(&name)) {
            continue;
        }
        let mut content = Vec::new();
        if file.read_to_end(&mut content).is_ok() {
            files.insert(name, content);
        }
    }
    files
}

/// Which channel of a texture a shader input reads: a `UsdUVTexture`
/// output's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Channel {
    Red,
    Green,
    Blue,
    Alpha,
    /// `rgb` (or anything else): the color, or red for a number.
    Color,
}

impl Channel {
    pub(crate) fn from_output(name: &str) -> Self {
        match name.rsplit(':').next().unwrap_or(name) {
            "r" => Channel::Red,
            "g" => Channel::Green,
            "b" => Channel::Blue,
            "a" => Channel::Alpha,
            _ => Channel::Color,
        }
    }

    /// The index of the channel a number is read from.
    fn index(self) -> usize {
        match self {
            Channel::Red | Channel::Color => 0,
            Channel::Green => 1,
            Channel::Blue => 2,
            Channel::Alpha => 3,
        }
    }
}

/// `bytes` as an image glTF can hold: kept as it is when it's a PNG or
/// JPEG, decoded and saved as a PNG otherwise (TGA, BMP, EXR…). `None` if
/// it can't be decoded.
pub(crate) fn embed(name: &str, bytes: Vec<u8>) -> Option<EmbeddedImage> {
    match image::guess_format(&bytes) {
        Ok(ImageFormat::Png) => Some(EmbeddedImage {
            name: name.to_owned(),
            mime: "image/png",
            bytes,
        }),
        Ok(ImageFormat::Jpeg) => Some(EmbeddedImage {
            name: name.to_owned(),
            mime: "image/jpeg",
            bytes,
        }),
        _ => {
            let image = image::load_from_memory(&bytes).ok()?;
            Some(png(&with_extension(name, "png"), &image.to_rgba8()))
        }
    }
}

/// Metalness and roughness, each a number or a channel of an image, as one
/// glTF metallic-roughness image (roughness green, metalness blue).
pub(crate) fn pack_metal_rough(
    name: &str,
    metal: Option<(&[u8], Channel)>,
    rough: Option<(&[u8], Channel)>,
) -> Option<EmbeddedImage> {
    let metal = metal.and_then(|(bytes, channel)| Some((decode(bytes)?, channel)));
    let rough = rough.and_then(|(bytes, channel)| Some((decode(bytes)?, channel)));
    let size = largest([metal.as_ref(), rough.as_ref()].into_iter().flatten().map(|(image, _)| image))?;
    let metal = metal.map(|(image, channel)| (fit(image, size), channel));
    let rough = rough.map(|(image, channel)| (fit(image, size), channel));
    let packed = RgbaImage::from_fn(size.0, size.1, |x, y| {
        // A factor of 1 times the image: 255 where a map is missing, so the
        // material's own number (put in its factor) is what counts.
        let read = |map: &Option<(RgbaImage, Channel)>| {
            map.as_ref()
                .map_or(255, |(image, channel)| image.get_pixel(x, y)[channel.index()])
        };
        image::Rgba([0, read(&rough), read(&metal), 255])
    });
    Some(png(name, &packed))
}

/// The base color image (or a plain `color` where there's none) with
/// `opacity`'s channel as its alpha.
pub(crate) fn with_opacity(
    name: &str,
    base: Option<&[u8]>,
    color: [f32; 3],
    opacity: (&[u8], Channel),
) -> Option<EmbeddedImage> {
    let alpha = decode(opacity.0)?;
    let base = base.and_then(decode);
    let size = largest([Some(&alpha), base.as_ref()].into_iter().flatten())?;
    let alpha = fit(alpha, size);
    let base = base.map(|base| fit(base, size));
    let plain = color.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8);
    let combined = RgbaImage::from_fn(size.0, size.1, |x, y| {
        let [r, g, b] = match &base {
            Some(base) => {
                let pixel = base.get_pixel(x, y);
                [pixel[0], pixel[1], pixel[2]]
            }
            None => plain,
        };
        image::Rgba([r, g, b, alpha.get_pixel(x, y)[opacity.1.index()]])
    });
    Some(png(name, &combined))
}

/// One channel of an image, as a grey image (for occlusion, which glTF
/// reads from red).
pub(crate) fn single_channel(name: &str, bytes: &[u8], channel: Channel) -> Option<EmbeddedImage> {
    let image = decode(bytes)?;
    let grey = RgbaImage::from_fn(image.width(), image.height(), |x, y| {
        let value = image.get_pixel(x, y)[channel.index()];
        image::Rgba([value, value, value, 255])
    });
    Some(png(name, &grey))
}

fn decode(bytes: &[u8]) -> Option<RgbaImage> {
    image::load_from_memory(bytes).ok().map(|image| image.to_rgba8())
}

fn largest<'a>(images: impl Iterator<Item = &'a RgbaImage>) -> Option<(u32, u32)> {
    images
        .map(|image| image.dimensions())
        .max_by_key(|(width, height)| u64::from(*width) * u64::from(*height))
}

fn fit(image: RgbaImage, size: (u32, u32)) -> RgbaImage {
    if image.dimensions() == size {
        return image;
    }
    DynamicImage::ImageRgba8(image)
        .resize_exact(size.0, size.1, image::imageops::FilterType::Triangle)
        .to_rgba8()
}

fn png(name: &str, image: &RgbaImage) -> EmbeddedImage {
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encoding a PNG in memory can't fail");
    EmbeddedImage {
        name: name.to_owned(),
        mime: "image/png",
        bytes: bytes.into_inner(),
    }
}

/// `name` with its extension changed (or added).
pub(crate) fn with_extension(name: &str, extension: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{stem}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny(pixels: [[u8; 4]; 4]) -> Vec<u8> {
        let image = RgbaImage::from_fn(2, 2, |x, y| image::Rgba(pixels[(y * 2 + x) as usize]));
        png("t.png", &image).bytes
    }

    #[test]
    fn metal_and_rough_maps_pack_into_blue_and_green() {
        let metal = tiny([[10, 0, 0, 255], [20, 0, 0, 255], [30, 0, 0, 255], [40, 0, 0, 255]]);
        let rough = tiny([[0, 0, 7, 255]; 4]);
        let packed = pack_metal_rough(
            "m.png",
            Some((&metal, Channel::Red)),
            Some((&rough, Channel::Blue)),
        )
        .unwrap();
        let image = decode(&packed.bytes).unwrap();
        assert_eq!(image.get_pixel(1, 0).0, [0, 7, 20, 255]);
        // Without a metal map, blue is 255 (the factor decides).
        let only = pack_metal_rough("m.png", None, Some((&rough, Channel::Blue))).unwrap();
        assert_eq!(decode(&only.bytes).unwrap().get_pixel(0, 0).0, [0, 7, 255, 255]);
        assert!(pack_metal_rough("m.png", None, None).is_none());
    }

    #[test]
    fn opacity_goes_into_the_colors_alpha() {
        let opacity = tiny([[0, 0, 0, 255], [255, 0, 0, 255], [128, 0, 0, 255], [64, 0, 0, 255]]);
        let combined =
            with_opacity("c.png", None, [1.0, 0.5, 0.0], (&opacity, Channel::Red)).unwrap();
        let image = decode(&combined.bytes).unwrap();
        assert_eq!(image.get_pixel(0, 1).0, [255, 128, 0, 128]);
    }

    #[test]
    fn png_and_jpeg_stay_as_they_are_and_others_become_png() {
        let png_bytes = tiny([[1, 2, 3, 255]; 4]);
        assert_eq!(embed("a.png", png_bytes.clone()).unwrap().bytes, png_bytes);
        let mut tga = Cursor::new(Vec::new());
        RgbaImage::from_pixel(2, 2, image::Rgba([9, 9, 9, 255]))
            .write_to(&mut tga, ImageFormat::Tga)
            .unwrap();
        let converted = embed("b.tga", tga.into_inner()).unwrap();
        assert_eq!((converted.name.as_str(), converted.mime), ("b.png", "image/png"));
        assert!(embed("c.png", b"not an image".to_vec()).is_none());
    }

    #[test]
    fn asset_paths_are_found_in_any_of_their_spellings() {
        let folder = std::env::temp_dir().join(format!("axcenity-convert-files-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("textures")).unwrap();
        std::fs::write(folder.join("textures/wood.png"), b"wood").unwrap();
        let files = Files::new(&folder.join("scene.usda"), &[]);
        for spelling in ["./textures/wood.png", "textures\\wood.png", "@textures/wood.png@"] {
            assert_eq!(
                files.read(spelling),
                Some(("wood.png".to_owned(), b"wood".to_vec())),
                "{spelling}"
            );
        }
        assert_eq!(files.read("textures/missing.png"), None);
        assert_eq!(Channel::from_output("outputs:g"), Channel::Green);
        assert_eq!(Channel::from_output("outputs:rgb"), Channel::Color);
    }
}
