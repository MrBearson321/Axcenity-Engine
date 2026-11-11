//! Choosing which variant to download, ported from the Django Toolbox's
//! `pick_bundle`: the wanted format first, else the next in a fallback
//! list, else whatever is there; then the wanted resolution, else the
//! nearest below, else the nearest above. Each place the choice differs
//! from what was asked gets a note to show.

use crate::record::Variant;

/// The variant chosen and why it differs from what was asked.
#[derive(Debug)]
pub struct Choice<'a> {
    pub variant: &'a Variant,
    /// Sentences such as "No 8K here, so 4K, the nearest." Empty when the
    /// variant is exactly what was asked for.
    pub notes: Vec<String>,
}

impl Choice<'_> {
    /// What Get will fetch, with the notes: "Gets 4K JPG. No 8K here, so 4K, the nearest."
    pub fn describe(&self) -> String {
        let mut text = format!("Gets {}.", self.variant.label);
        for note in &self.notes {
            text.push(' ');
            text.push_str(note);
        }
        text
    }
}

/// Picks from `variants` for a `resolution` (`2k`) and `format` (`jpg`),
/// trying `fallbacks` (formats, in order) when the format isn't there.
/// `None` only when there are no variants.
pub fn choose<'a>(
    variants: &'a [Variant],
    resolution: &str,
    format: &str,
    fallbacks: &[&str],
) -> Option<Choice<'a>> {
    let wanted = size(resolution);
    let mut order = vec![format];
    order.extend(fallbacks.iter().filter(|fallback| **fallback != format));
    let score = |variant: &Variant| {
        let format_rank = order
            .iter()
            .position(|known| variant.format.eq_ignore_ascii_case(known))
            .unwrap_or(order.len());
        let have = variant.resolution.as_deref().map_or(0, size);
        let distance = match (have, wanted) {
            (0, _) | (_, 0) => (0, 0),
            (have, wanted) if have <= wanted => (0, wanted - have),
            (have, wanted) => (1, have - wanted),
        };
        (format_rank, distance)
    };
    let variant = variants.iter().min_by_key(|variant| score(variant))?;
    let mut notes = Vec::new();
    // An unknown format (empty) gets no note: there's nothing to say it is.
    if !variant.format.is_empty() && !variant.format.eq_ignore_ascii_case(format) {
        notes.push(format!(
            "No {} here, so {}.",
            format.to_ascii_uppercase(),
            variant.format.to_ascii_uppercase()
        ));
    }
    if let Some(have) = &variant.resolution
        && wanted != 0
        && size(have) != wanted
    {
        notes.push(format!(
            "No {} here, so {}, the nearest.",
            resolution.to_ascii_uppercase(),
            have.to_ascii_uppercase()
        ));
    }
    Some(Choice { variant, notes })
}

/// `2k` → 2 (thousands of pixels); 0 when it isn't a resolution.
fn size(resolution: &str) -> u32 {
    resolution
        .trim()
        .to_ascii_lowercase()
        .strip_suffix('k')
        .and_then(|number| number.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Package;

    fn variant(resolution: &str, format: &str) -> Variant {
        Variant {
            label: format!("{} {}", resolution.to_uppercase(), format.to_uppercase()),
            resolution: Some(resolution.to_owned()),
            format: format.to_owned(),
            package: Package::Archive,
            files: Vec::new(),
        }
    }

    #[test]
    fn the_exact_package_needs_no_note() {
        let variants = [
            variant("1k", "jpg"),
            variant("2k", "jpg"),
            variant("2k", "png"),
        ];
        let choice = choose(&variants, "2k", "png", &["jpg"]).unwrap();
        assert_eq!(choice.variant.label, "2K PNG");
        assert!(choice.notes.is_empty());
        assert_eq!(choice.describe(), "Gets 2K PNG.");
    }

    #[test]
    fn a_missing_resolution_takes_the_nearest_below_then_above() {
        let variants = [
            variant("1k", "jpg"),
            variant("4k", "jpg"),
            variant("8k", "jpg"),
        ];
        let choice = choose(&variants, "2k", "jpg", &[]).unwrap();
        assert_eq!(choice.variant.label, "1K JPG");
        assert_eq!(choice.notes, ["No 2K here, so 1K, the nearest."]);
        let higher = [variant("4k", "jpg"), variant("8k", "jpg")];
        assert_eq!(
            choose(&higher, "2k", "jpg", &[]).unwrap().variant.label,
            "4K JPG"
        );
    }

    #[test]
    fn the_format_comes_before_the_resolution() {
        let variants = [variant("2k", "png"), variant("1k", "jpg")];
        let choice = choose(&variants, "2k", "jpg", &["png"]).unwrap();
        assert_eq!(choice.variant.label, "1K JPG");
        let only_png = [variant("2k", "png")];
        let choice = choose(&only_png, "2k", "jpg", &["png"]).unwrap();
        assert_eq!(choice.notes, ["No JPG here, so PNG."]);
        assert!(choose(&[], "2k", "jpg", &[]).is_none());
    }
}
