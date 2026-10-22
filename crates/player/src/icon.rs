//! The window icon: the engine's logo, built into the binary.
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
    let icon = decode(ICON_PNG).and_then(|(rgba, width, height)| {
        Icon::from_rgba(rgba, width, height).context("the window icon was rejected")
    });
    match icon {
        Ok(icon) => Some(icon),
        Err(err) => {
            log::warn!("no window icon: {err:#}");
            None
        }
    }
}

/// Decodes a PNG into RGBA bytes (8 bits per channel), its width and height.
pub(crate) fn decode(png_bytes: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    // Palette and 16-bit images become plain 8-bit channels.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().context("not a PNG file")?;
    let mut rgba = vec![0; reader.output_buffer_size().context("the PNG is too big")?];
    let info = reader.next_frame(&mut rgba).context("a broken PNG file")?;
    if info.color_type != png::ColorType::Rgba {
        bail!("the PNG has no alpha channel ({:?})", info.color_type);
    }
    rgba.truncate(info.buffer_size());
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
}
