//! The game's icon in a build (cohesion pass CW8.2): a Windows `.ico`, a
//! macOS `.icns` and a PNG for Linux, each made from the project's
//! `branding.icon` at the sizes each system asks for. Both containers hold
//! PNG images, which Windows (Vista on) and macOS (10.7 on) read.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, imageops::FilterType};

/// The sizes a Windows `.ico` holds, smallest first.
const ICO_SIZES: [u32; 6] = [16, 32, 48, 64, 128, 256];
/// The `.icns` types for PNG images and their sizes.
const ICNS_TYPES: [(&[u8; 4], u32); 7] = [
    (b"icp4", 16),
    (b"icp5", 32),
    (b"icp6", 64),
    (b"ic07", 128),
    (b"ic08", 256),
    (b"ic09", 512),
    (b"ic10", 1024),
];
/// The Linux icon's side, or the picture's when it's smaller.
const LINUX_SIZE: u32 = 256;

/// The game's icon, decoded once.
pub(super) struct GameIcon {
    image: DynamicImage,
}

impl GameIcon {
    /// The icon in the PNG `bytes`.
    pub(super) fn read(bytes: &[u8]) -> Result<GameIcon, String> {
        image::load_from_memory_with_format(bytes, ImageFormat::Png)
            .map(|image| GameIcon { image })
            .map_err(|err| format!("the icon can't be read: {err}"))
    }

    fn side(&self) -> u32 {
        self.image.width().min(self.image.height())
    }

    /// The icon `side` pixels square, as a PNG.
    fn png(&self, side: u32) -> Vec<u8> {
        let image = if self.image.width() == side && self.image.height() == side {
            self.image.clone()
        } else {
            self.image.resize_exact(side, side, FilterType::Lanczos3)
        };
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, ImageFormat::Png)
            .expect("writing a PNG to memory can't fail");
        bytes.into_inner()
    }

    /// The sizes from `wanted` this icon makes without growing (the
    /// smallest one always, so there's at least one).
    fn sizes<'a, T: Copy>(&self, wanted: &'a [(T, u32)]) -> impl Iterator<Item = (T, u32)> + 'a {
        let side = self.side();
        let smallest = wanted.first().map_or(0, |&(_, size)| size);
        wanted
            .iter()
            .copied()
            .filter(move |&(_, size)| size <= side || size == smallest)
    }

    /// A Windows `.ico`: a header, one entry per size, then the PNGs.
    pub(super) fn ico(&self) -> Vec<u8> {
        let wanted: Vec<((), u32)> = ICO_SIZES.iter().map(|&size| ((), size)).collect();
        let images: Vec<(u32, Vec<u8>)> = self
            .sizes(&wanted)
            .map(|((), size)| (size, self.png(size)))
            .collect();
        let mut ico = Vec::new();
        ico.extend(0u16.to_le_bytes()); // reserved
        ico.extend(1u16.to_le_bytes()); // an icon, not a cursor
        ico.extend((images.len() as u16).to_le_bytes());
        let mut offset = 6 + 16 * images.len() as u32;
        for (size, png) in &images {
            // 256 is written as 0: the field is one byte.
            let side = if *size >= 256 { 0 } else { *size as u8 };
            ico.extend([side, side, 0, 0]);
            ico.extend(1u16.to_le_bytes()); // colour planes
            ico.extend(32u16.to_le_bytes()); // bits per pixel
            ico.extend((png.len() as u32).to_le_bytes());
            ico.extend(offset.to_le_bytes());
            offset += png.len() as u32;
        }
        for (_, png) in images {
            ico.extend(png);
        }
        ico
    }

    /// A macOS `.icns`: `icns`, the file's length, then each image as its
    /// type, its length (with these 8 bytes) and its PNG.
    pub(super) fn icns(&self) -> Vec<u8> {
        let mut body = Vec::new();
        for (kind, size) in self.sizes(&ICNS_TYPES) {
            let png = self.png(size);
            body.extend(kind);
            body.extend((png.len() as u32 + 8).to_be_bytes());
            body.extend(png);
        }
        let mut icns = b"icns".to_vec();
        icns.extend((body.len() as u32 + 8).to_be_bytes());
        icns.extend(body);
        icns
    }

    /// The Linux icon: a PNG at most 256 pixels square.
    pub(super) fn linux_png(&self) -> Vec<u8> {
        self.png(self.side().min(LINUX_SIZE))
    }
}

/// `text` safe inside a property list's `<string>`.
pub(super) fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `text` as a desktop entry's value: one line, backslashes doubled.
pub(super) fn desktop_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace(['\n', '\r', '\t'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(side: u32) -> GameIcon {
        GameIcon {
            image: DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                side,
                side,
                image::Rgba([200, 40, 40, 255]),
            )),
        }
    }

    /// Each image in an `.ico`: its side (0 is 256) and its PNG.
    fn ico_images(ico: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
        (0..count)
            .map(|index| {
                let entry = &ico[6 + 16 * index..6 + 16 * (index + 1)];
                let side = if entry[0] == 0 { 256 } else { entry[0] as u32 };
                let length = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
                let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
                (side, ico[offset..offset + length].to_vec())
            })
            .collect()
    }

    #[test]
    fn the_ico_holds_each_size_up_to_the_pictures_as_pngs() {
        let ico = icon(256).ico();
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        let images = ico_images(&ico);
        let sides: Vec<u32> = images.iter().map(|(side, _)| *side).collect();
        assert_eq!(sides, ICO_SIZES);
        for (side, png) in images {
            let decoded = image::load_from_memory(&png).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (side, side));
        }
        // A small picture isn't grown.
        let sides: Vec<u32> = ico_images(&icon(40).ico()).iter().map(|i| i.0).collect();
        assert_eq!(sides, [16, 32]);
        let sides: Vec<u32> = ico_images(&icon(12).ico()).iter().map(|i| i.0).collect();
        assert_eq!(sides, [16], "the smallest, at least");
    }

    #[test]
    fn the_icns_lists_png_images_with_their_lengths() {
        let icns = icon(512).icns();
        assert_eq!(&icns[..4], b"icns");
        assert_eq!(
            u32::from_be_bytes(icns[4..8].try_into().unwrap()) as usize,
            icns.len()
        );
        let mut kinds = Vec::new();
        let mut at = 8;
        while at < icns.len() {
            let kind = std::str::from_utf8(&icns[at..at + 4]).unwrap().to_owned();
            let length = u32::from_be_bytes(icns[at + 4..at + 8].try_into().unwrap()) as usize;
            assert!(icns[at + 8..at + length].starts_with(b"\x89PNG"));
            kinds.push(kind);
            at += length;
        }
        assert_eq!(at, icns.len());
        assert_eq!(kinds, ["icp4", "icp5", "icp6", "ic07", "ic08", "ic09"]);
    }

    #[test]
    fn names_are_made_safe_for_the_plist_and_the_desktop_entry() {
        assert_eq!(xml_text("Cats & <Dogs>"), "Cats &amp; &lt;Dogs&gt;");
        assert_eq!(desktop_text("A\\B\nC"), "A\\\\B C");
        let linux = image::load_from_memory(&icon(1024).linux_png()).unwrap();
        assert_eq!((linux.width(), linux.height()), (256, 256));
    }
}
