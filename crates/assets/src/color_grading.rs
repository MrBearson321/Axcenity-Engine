//! Color grading files a `ColorCorrectionEffect` names: a lookup table
//! (`LookupTable`) that maps every color to another, and a palette
//! (`Palette`) every color is snapped to.
//!
//! A lookup table is an Adobe/Resolve `.cube` file or a strip image: N
//! squares of N × N pixels side by side (such as 256 × 16 or 1024 × 32).
//! Square `b` holds blue `b / (N - 1)`; in it red grows to the right and
//! green downward. A palette is any image: its colors are the palette.
//! Colors in both are sRGB-encoded, as a screen shows them.

use std::path::Path;
use std::sync::Arc;

use crate::model::{decode_image, new_key};
use crate::{Image, safe_path};

/// The file extensions a lookup table can have: a `.cube` file, or a strip
/// image.
pub const LOOKUP_EXTENSIONS: [&str; 6] = ["cube", "png", "jpg", "jpeg", "tga", "bmp"];
/// Where lookup tables and palettes picked from outside the project are
/// copied to.
pub const PROJECT_GRADING_FOLDER: &str = "assets/grading";
/// The most colors a palette keeps: the first this many different ones.
pub const MAX_PALETTE_COLORS: usize = 64;
/// The largest lookup table, in entries along each side.
pub const MAX_LOOKUP_SIZE: u32 = 128;

/// A color lookup table: a cube of `size` entries a side, mapping each
/// sRGB color to another.
#[derive(Clone)]
pub struct ColorLookup {
    key: u64,
    pub size: u32,
    /// Red, green and blue for each entry, sRGB-encoded, red changing
    /// fastest, then green, then blue (the `.cube` order). Shared, so
    /// handing it to the renderer each frame copies nothing.
    pub rgb: Arc<Vec<f32>>,
}

impl ColorLookup {
    /// Different for every lookup table read.
    pub fn key(&self) -> u64 {
        self.key
    }

    /// The table that changes nothing, `size` entries a side.
    pub fn neutral(size: u32) -> ColorLookup {
        let size = size.clamp(2, MAX_LOOKUP_SIZE);
        let step = |i: u32| i as f32 / (size - 1) as f32;
        let mut rgb = Vec::with_capacity((size * size * size * 3) as usize);
        for blue in 0..size {
            for green in 0..size {
                for red in 0..size {
                    rgb.extend([step(red), step(green), step(blue)]);
                }
            }
        }
        ColorLookup {
            key: new_key(),
            size,
            rgb: Arc::new(rgb),
        }
    }
}

impl std::fmt::Debug for ColorLookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size = self.size;
        write!(f, "ColorLookup #{} ({size}×{size}×{size})", self.key)
    }
}

/// A palette's colors, sRGB-encoded, in the order they were first found.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub colors: Arc<Vec<[f32; 3]>>,
}

/// Reads the lookup table at `path` inside `folder` (a `.cube` file or a
/// strip image), or explains why it can't. Not cached:
/// [`Assets::color_lookup`](crate::Assets::color_lookup) is the cached way.
pub fn read_color_lookup(folder: &Path, path: &str) -> Result<ColorLookup, String> {
    let bytes = read(folder, path, "a lookup table", "assets/grading/warm.cube")?;
    if path.to_ascii_lowercase().ends_with(".cube") {
        let text = String::from_utf8(bytes).map_err(|_| "it isn't a text file".to_owned())?;
        parse_cube(&text)
    } else {
        lookup_from_strip(&decode_image(&bytes, path)?)
    }
}

/// Reads the palette image at `path` inside `folder`, or explains why it
/// can't. Not cached: [`Assets::palette`](crate::Assets::palette) is the
/// cached way. The second value warns that colors were left out.
pub fn read_palette(folder: &Path, path: &str) -> Result<(Palette, Option<String>), String> {
    let bytes = read(folder, path, "a palette", "assets/grading/retro.png")?;
    Ok(palette_from_image(&decode_image(&bytes, path)?))
}

fn read(folder: &Path, path: &str, what: &str, example: &str) -> Result<Vec<u8>, String> {
    let relative = safe_path(path).ok_or_else(|| {
        format!("{what} must be a file inside the project folder, such as \"{example}\"")
    })?;
    std::fs::read(folder.join(relative)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })
}

/// A `.cube` file's 3D table. `TITLE`, comments (`#`) and blank lines are
/// skipped; `DOMAIN_MIN` and `DOMAIN_MAX` must be 0 and 1 if given.
pub fn parse_cube(text: &str) -> Result<ColorLookup, String> {
    let mut size = None;
    let mut rgb = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |message: String| format!("line {}: {message}", number + 1);
        let mut words = line.split_whitespace();
        let first = words.next().unwrap_or_default();
        match first {
            "TITLE" => continue,
            "LUT_1D_SIZE" => {
                return Err(at(
                    "it's a 1D table; only 3D tables (LUT_3D_SIZE) can be used".to_owned(),
                ));
            }
            "LUT_3D_SIZE" => {
                let value = words
                    .next()
                    .and_then(|word| word.parse::<u32>().ok())
                    .filter(|size| (2..=MAX_LOOKUP_SIZE).contains(size))
                    .ok_or_else(|| {
                        at(format!(
                            "LUT_3D_SIZE must be a whole number from 2 to {MAX_LOOKUP_SIZE}"
                        ))
                    })?;
                size = Some(value);
                continue;
            }
            "DOMAIN_MIN" | "DOMAIN_MAX" => {
                let expected = if first == "DOMAIN_MIN" { 0.0 } else { 1.0 };
                let values: Vec<f32> = words.filter_map(|word| word.parse().ok()).collect();
                if values.len() != 3 || values.iter().any(|&value| value != expected) {
                    return Err(at(format!(
                        "only a domain from 0 to 1 can be used ({first} {expected} {expected} {expected})"
                    )));
                }
                continue;
            }
            _ => {}
        }
        if first.starts_with(|c: char| c.is_ascii_alphabetic()) {
            // Another keyword, such as LUT_3D_INPUT_RANGE: nothing to use.
            continue;
        }
        let values: Vec<f32> = line
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| at(format!("`{line}` isn't three numbers")))?;
        let [r, g, b] = values[..] else {
            return Err(at(format!("`{line}` isn't three numbers")));
        };
        if ![r, g, b].iter().all(|value| value.is_finite()) {
            return Err(at(format!("`{line}` isn't three numbers")));
        }
        rgb.extend([r, g, b]);
    }
    let size = size.ok_or("it has no LUT_3D_SIZE line")?;
    let wanted = (size * size * size) as usize;
    if rgb.len() / 3 != wanted {
        return Err(format!(
            "LUT_3D_SIZE {size} needs {wanted} colors, but it has {}",
            rgb.len() / 3
        ));
    }
    Ok(ColorLookup {
        key: new_key(),
        size,
        rgb: Arc::new(rgb),
    })
}

/// A strip image's table: `N` squares of `N` × `N` pixels side by side.
pub fn lookup_from_strip(image: &Image) -> Result<ColorLookup, String> {
    let size = image.height;
    if image.width != size * size || !(2..=MAX_LOOKUP_SIZE).contains(&size) {
        return Err(format!(
            "a lookup strip is N squares of N × N pixels side by side (such as 256 × 16), \
             with N from 2 to {MAX_LOOKUP_SIZE}, but it's {} × {}",
            image.width, image.height
        ));
    }
    let mut rgb = Vec::with_capacity((size * size * size * 3) as usize);
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                let x = blue * size + red;
                let pixel = ((green * image.width + x) * 4) as usize;
                rgb.extend(
                    image.rgba()[pixel..pixel + 3]
                        .iter()
                        .map(|&byte| f32::from(byte) / 255.0),
                );
            }
        }
    }
    Ok(ColorLookup {
        key: new_key(),
        size,
        rgb: Arc::new(rgb),
    })
}

/// The different colors in `image`, row by row from the top, up to
/// [`MAX_PALETTE_COLORS`] (with a warning if it has more). Fully
/// transparent pixels are left out.
pub fn palette_from_image(image: &Image) -> (Palette, Option<String>) {
    let mut seen: Vec<[u8; 3]> = Vec::new();
    let mut more = false;
    for pixel in image.rgba().as_chunks::<4>().0 {
        let color = [pixel[0], pixel[1], pixel[2]];
        if pixel[3] == 0 || seen.contains(&color) {
            continue;
        }
        if seen.len() == MAX_PALETTE_COLORS {
            more = true;
            break;
        }
        seen.push(color);
    }
    let colors = seen
        .iter()
        .map(|color| color.map(|byte| f32::from(byte) / 255.0))
        .collect();
    let warning = more.then(|| {
        format!(
            "has more than {MAX_PALETTE_COLORS} colors; only the first {MAX_PALETTE_COLORS} are used"
        )
    });
    (
        Palette {
            colors: Arc::new(colors),
        },
        warning,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(size: u32, colors: &[[f32; 3]]) -> String {
        let mut text = format!("# made by a test\nTITLE \"Test\"\nLUT_3D_SIZE {size}\n\n");
        for [r, g, b] in colors {
            text.push_str(&format!("{r} {g} {b}\n"));
        }
        text
    }

    fn colors(lookup: &ColorLookup) -> Vec<[f32; 3]> {
        lookup
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[r, g, b]| [r, g, b])
            .collect()
    }

    #[test]
    fn a_cube_file_reads_red_fastest_and_skips_titles_and_comments() {
        let neutral = ColorLookup::neutral(2);
        let text = cube(2, &colors(&neutral)).replace(
            "LUT_3D_SIZE 2",
            "LUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1.0 1.0 1.0",
        );
        let lookup = parse_cube(&text).unwrap();
        assert_eq!(lookup.size, 2);
        assert_eq!(colors(&lookup), colors(&neutral));
        assert_eq!(colors(&lookup)[1], [1.0, 0.0, 0.0], "red changes first");
        assert_eq!(colors(&lookup)[4], [0.0, 0.0, 1.0], "then green, then blue");
    }

    #[test]
    fn broken_cube_files_say_what_is_wrong() {
        let error = |text: &str| parse_cube(text).unwrap_err();
        assert!(error("0 0 0\n").contains("no LUT_3D_SIZE"));
        assert!(error("LUT_3D_SIZE 1\n").contains("from 2 to"));
        assert!(error("LUT_1D_SIZE 256\n").contains("1D"));
        assert!(error("LUT_3D_SIZE 2\n0 0 0\n").contains("needs 8 colors, but it has 1"));
        assert!(error("LUT_3D_SIZE 2\n0 zero 0\n").starts_with("line 2:"));
        assert!(error("LUT_3D_SIZE 2\n0 0\n").contains("isn't three numbers"));
        assert!(error("DOMAIN_MAX 2 2 2\nLUT_3D_SIZE 2\n").contains("from 0 to 1"));
    }

    #[test]
    fn a_neutral_strip_image_reads_as_the_neutral_table() {
        let size = 4;
        let mut rgba = Vec::new();
        for green in 0..size {
            for blue in 0..size {
                for red in 0..size {
                    let level = |i: u32| (i * 255 / (size - 1)) as u8;
                    rgba.extend([level(red), level(green), level(blue), 255]);
                }
            }
        }
        let strip = Image::new(size * size, size, rgba);
        let lookup = lookup_from_strip(&strip).unwrap();
        assert_eq!(lookup.size, size);
        assert_eq!(colors(&lookup), colors(&ColorLookup::neutral(size)));

        let square = Image::new(16, 16, vec![0; 16 * 16 * 4]);
        assert!(lookup_from_strip(&square).unwrap_err().contains("16 × 16"));
    }

    #[test]
    fn a_palette_is_the_images_different_colors_in_order() {
        let pixels = [
            [255, 0, 0, 255],
            [0, 0, 0, 255],
            [255, 0, 0, 255],
            [9, 9, 9, 0],
            [255, 255, 255, 255],
        ];
        let image = Image::new(5, 1, pixels.concat());
        let (palette, warning) = palette_from_image(&image);
        assert_eq!(
            *palette.colors,
            vec![[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]
        );
        assert_eq!(warning, None);

        let many: Vec<u8> = (0..100u8).flat_map(|i| [i, i, i, 255]).collect();
        let (palette, warning) = palette_from_image(&Image::new(100, 1, many));
        assert_eq!(palette.colors.len(), MAX_PALETTE_COLORS);
        assert!(warning.unwrap().contains("more than 64"));
    }
}
