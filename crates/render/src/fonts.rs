//! The fonts game UI text can use: the ones built into the engine, and a
//! project's own `.ttf` and `.otf` files, which come in the draw list
//! ([`DrawList::fonts`](crate::DrawList)) and are loaded the first time
//! they're seen. CSS names a font by its family, such as `Poppins`.

use std::sync::Arc;

use glyphon::cosmic_text::fontdb;

/// A font built into the engine, by the family name CSS uses for it.
#[derive(Clone, Copy, Debug)]
pub struct BuiltInFont {
    pub family: &'static str,
    /// What kind of font it is, for the font picker.
    pub kind: &'static str,
    data: &'static [u8],
}

/// Built in, in the order the font picker lists them. Each is under the SIL
/// Open Font License (its `*-OFL.txt` is next to it in `assets/fonts`).
/// Montserrat is the default; JetBrains Mono is `monospace` and Lora `serif`.
pub const BUILT_IN_FONTS: [BuiltInFont; 12] = [
    font(
        "Montserrat",
        "sans-serif",
        include_bytes!("../../../assets/fonts/Montserrat-Regular.ttf"),
    ),
    font(
        "Inter",
        "sans-serif",
        include_bytes!("../../../assets/fonts/Inter-Regular.ttf"),
    ),
    font(
        "Roboto",
        "sans-serif",
        include_bytes!("../../../assets/fonts/Roboto-Variable.ttf"),
    ),
    font(
        "Poppins",
        "sans-serif",
        include_bytes!("../../../assets/fonts/Poppins-Regular.ttf"),
    ),
    font(
        "Nunito",
        "rounded",
        include_bytes!("../../../assets/fonts/Nunito-Variable.ttf"),
    ),
    font(
        "Oswald",
        "condensed",
        include_bytes!("../../../assets/fonts/Oswald-Variable.ttf"),
    ),
    font(
        "Bebas Neue",
        "display",
        include_bytes!("../../../assets/fonts/BebasNeue-Regular.ttf"),
    ),
    font(
        "Bangers",
        "comic",
        include_bytes!("../../../assets/fonts/Bangers-Regular.ttf"),
    ),
    font(
        "Lobster",
        "script",
        include_bytes!("../../../assets/fonts/Lobster-Regular.ttf"),
    ),
    font(
        "Press Start 2P",
        "pixel",
        include_bytes!("../../../assets/fonts/PressStart2P-Regular.ttf"),
    ),
    font(
        "Lora",
        "serif",
        include_bytes!("../../../assets/fonts/Lora-Variable.ttf"),
    ),
    font(
        "JetBrains Mono",
        "monospace",
        include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"),
    ),
];

const fn font(family: &'static str, kind: &'static str, data: &'static [u8]) -> BuiltInFont {
    BuiltInFont { family, kind, data }
}

/// The bold, italic and bold italic faces of the built-in families that
/// have them (SIL OFL, from the same projects). Each joins its family by the
/// name inside it. The `-Variable` italics are drawn at their regular
/// weight, so their bold is thickened, as Roboto, Nunito, Oswald and Lora's
/// upright bold is.
const VARIANTS: [&[u8]; 15] = [
    include_bytes!("../../../assets/fonts/Montserrat-Bold.ttf"),
    include_bytes!("../../../assets/fonts/Montserrat-Italic.ttf"),
    include_bytes!("../../../assets/fonts/Montserrat-BoldItalic.ttf"),
    include_bytes!("../../../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../../../assets/fonts/Inter-Italic.ttf"),
    include_bytes!("../../../assets/fonts/Inter-BoldItalic.ttf"),
    include_bytes!("../../../assets/fonts/Poppins-Bold.ttf"),
    include_bytes!("../../../assets/fonts/Poppins-Italic.ttf"),
    include_bytes!("../../../assets/fonts/Poppins-BoldItalic.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    include_bytes!("../../../assets/fonts/Roboto-Italic-Variable.ttf"),
    include_bytes!("../../../assets/fonts/Nunito-Italic-Variable.ttf"),
    include_bytes!("../../../assets/fonts/Lora-Italic-Variable.ttf"),
];

/// A font file from the project, for UI text.
#[derive(Clone, Debug)]
pub struct UiFont {
    /// Its project-relative path, which tells fonts apart.
    pub path: String,
    pub data: Arc<Vec<u8>>,
}

/// Every built-in face's data, regular and variants, for loading into the
/// text system.
pub(crate) fn built_in_data() -> impl Iterator<Item = &'static [u8]> {
    BUILT_IN_FONTS
        .iter()
        .map(|font| font.data)
        .chain(VARIANTS.iter().copied())
}

/// A font database with only the built-in faces, without the system's.
#[cfg(test)]
pub(crate) fn built_in_database() -> fontdb::Database {
    let mut db = fontdb::Database::new();
    for data in built_in_data() {
        db.load_font_data(data.to_vec());
    }
    db
}

/// The family names in a font file (usually one), as CSS names them. Empty
/// when it isn't a font that can be read.
pub fn font_families(data: &[u8]) -> Vec<String> {
    let mut db = fontdb::Database::new();
    db.load_font_data(data.to_vec());
    let mut families: Vec<String> = Vec::new();
    for face in db.faces() {
        for (name, _) in &face.families {
            if !families.contains(name) {
                families.push(name.clone());
            }
        }
    }
    families
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_font_reads_as_the_family_it_is_listed_as() {
        for font in BUILT_IN_FONTS {
            assert_eq!(
                font_families(font.data),
                vec![font.family.to_owned()],
                "{}",
                font.family
            );
        }
        assert!(font_families(b"not a font").is_empty());
    }

    #[test]
    fn every_variant_joins_a_built_in_family() {
        for data in VARIANTS {
            let families = font_families(data);
            assert_eq!(families.len(), 1, "{families:?}");
            assert!(
                BUILT_IN_FONTS.iter().any(|font| font.family == families[0]),
                "{families:?}"
            );
        }
    }
}
