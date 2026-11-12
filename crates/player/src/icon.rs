//! The window icon: the game's own (the manifest's `branding.icon`), or
//! the engine's logo, built into the binary, when it has none.
//!
//! Windows and X11 show it in the title bar and taskbar. macOS and Wayland
//! ignore window icons; there the app bundle or desktop file sets the icon,
//! which comes with exporting.

use anyhow::{Context, Result, bail};
use winit::window::Icon;

/// The logo at 256×256, from `assets/branding` (see its README).
/// `include_bytes!` copies the file into the program when it's compiled.
const ICON_PNG: &[u8] = include_bytes!("../../../assets/branding/icon-256.png");

/// The logo as a window icon, or `None` if it can't be used. A missing icon
/// isn't worth stopping the game for, so the reason only goes to the log.
pub fn window_icon() -> Option<Icon> {
    match icon_from(ICON_PNG) {
        Ok(icon) => Some(icon),
        Err(err) => {
            log::warn!("no window icon: {err:#}");
            None
        }
    }
}

/// The game's icon (a PNG's bytes) as the window icon, or Axcenity's when
/// it has none or it can't be used (said in the log).
pub fn game_icon(png: Option<&[u8]>) -> Option<Icon> {
    match png.map(icon_from) {
        Some(Ok(icon)) => Some(icon),
        Some(Err(err)) => {
            log::warn!("the game's icon can't be used, so Axcenity's is: {err:#}");
            window_icon()
        }
        None => window_icon(),
    }
}

fn icon_from(png_bytes: &[u8]) -> Result<Icon> {
    let (rgba, width, height) = decode(png_bytes)?;
    Icon::from_rgba(rgba, width, height).context("the window icon was rejected")
}

/// Decodes a PNG into RGBA bytes (8 bits per channel), its width and height.
pub(crate) fn decode(png_bytes: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    // Palette and 16-bit images become plain 8-bit channels.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().context("not a PNG file")?;
    let mut rgba = vec![0; reader.output_buffer_size().context("the PNG is too big")?];
    let info = reader.next_frame(&mut rgba).context("a broken PNG file")?;
    rgba.truncate(info.buffer_size());
    match info.color_type {
        png::ColorType::Rgba => {}
        // A game's icon may have no alpha channel: it's solid.
        png::ColorType::Rgb => {
            rgba = rgba
                .chunks_exact(3)
                .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
                .collect();
        }
        other => bail!("the PNG isn't in colour ({other:?})"),
    }
    Ok((rgba, info.width, info.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_decodes_into_a_window_icon() {
        let (rgba, width, height) = decode(ICON_PNG).unwrap();
        assert_eq!((width, height), (256, 256));
        assert_eq!(rgba.len(), 256 * 256 * 4);
        // The tile's rounded corners are see-through; its middle is solid.
        assert_eq!(rgba[3], 0);
        let middle = (128 * 256 + 128) * 4;
        assert_eq!(rgba[middle + 3], 255);
        assert!(window_icon().is_some());
    }

    #[test]
    fn a_games_icon_is_used_and_a_bad_one_falls_back_to_the_logo() {
        // A 2×2 solid red PNG with no alpha channel.
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255, 0, 0].repeat(4))
            .unwrap();
        let (rgba, width, height) = decode(&bytes).unwrap();
        assert_eq!((width, height), (2, 2));
        assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
        assert!(game_icon(Some(&bytes)).is_some());
        assert!(game_icon(Some(b"not a png")).is_some(), "the logo");
        assert!(game_icon(None).is_some());
    }
}
