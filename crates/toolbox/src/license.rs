//! Licence codes. Sources write licences many ways ("CC0 1.0", "CC BY 4.0",
//! Openverse's `by-nc-sa`, "Public Domain Mark"); records keep one
//! normalised code, the Blender pipeline's, plus the version and the deed's
//! address as the source gave them. Not legal advice.

/// Public-domain dedication; no credit needed.
pub const CC0: &str = "CC0";
/// Public domain (the Public Domain Mark).
pub const PD: &str = "PD";
pub const CC_BY: &str = "CC-BY";
pub const CC_BY_SA: &str = "CC-BY-SA";
pub const CC_BY_ND: &str = "CC-BY-ND";
pub const CC_BY_NC: &str = "CC-BY-NC";
pub const CC_BY_NC_SA: &str = "CC-BY-NC-SA";
pub const CC_BY_NC_ND: &str = "CC-BY-NC-ND";
/// Creative Commons Sampling Plus (retired; still on older sounds).
pub const SAMPLING_PLUS: &str = "SAMPLING+";
/// Non-commercial Sampling Plus.
pub const NC_SAMPLING_PLUS: &str = "NC-SAMPLING+";
/// Anything not recognised. Never treated as safe to use.
pub const UNKNOWN: &str = "unknown";

/// The CC0 deed.
pub const CC0_URL: &str = "https://creativecommons.org/publicdomain/zero/1.0/";

/// Openverse's licence codes and ours, both ways.
const OPENVERSE: [(&str, &str); 10] = [
    ("cc0", CC0),
    ("pdm", PD),
    ("by", CC_BY),
    ("by-sa", CC_BY_SA),
    ("by-nd", CC_BY_ND),
    ("by-nc", CC_BY_NC),
    ("by-nc-sa", CC_BY_NC_SA),
    ("by-nc-nd", CC_BY_NC_ND),
    ("sampling+", SAMPLING_PLUS),
    ("nc-sampling+", NC_SAMPLING_PLUS),
];

/// Our code for an Openverse licence code (`by-nc` → `CC-BY-NC`).
pub fn from_openverse(code: &str) -> &'static str {
    let code = code.trim().to_ascii_lowercase();
    OPENVERSE
        .iter()
        .find(|(theirs, _)| *theirs == code)
        .map_or(UNKNOWN, |(_, ours)| ours)
}

/// Openverse's code for one of ours, if it has one.
pub fn to_openverse(code: &str) -> Option<&'static str> {
    OPENVERSE
        .iter()
        .find(|(_, ours)| ours.eq_ignore_ascii_case(code))
        .map(|(theirs, _)| *theirs)
}

/// Our code for a licence however it's written: "CC0 1.0", "CC BY-NC-SA
/// 3.0", "Attribution 4.0 International", "Public Domain Mark", a deed's
/// address, or one of our codes. Anything else is [`UNKNOWN`].
pub fn normalize(text: &str) -> &'static str {
    let lower = text.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return UNKNOWN;
    }
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '+')
        .filter(|word| !word.is_empty())
        .collect();
    let has = |word: &str| words.contains(&word);
    if has("cc0") || lower.contains("publicdomain/zero") || lower.contains("cc zero") {
        return CC0;
    }
    if has("pd")
        || has("pdm")
        || lower.contains("public domain")
        || lower.contains("publicdomain/mark")
    {
        return PD;
    }
    if has("sampling+") || lower.contains("sampling plus") {
        return if has("nc") {
            NC_SAMPLING_PLUS
        } else {
            SAMPLING_PLUS
        };
    }
    let creative_commons = has("cc")
        || (has("creative") && has("commons"))
        || lower.contains("creativecommons.org/licenses/")
        || lower.starts_with("by");
    if !creative_commons || !(has("by") || has("attribution")) {
        return UNKNOWN;
    }
    let non_commercial = has("nc") || has("noncommercial");
    let share_alike = has("sa") || has("sharealike");
    let no_derivatives = has("nd") || has("noderivs") || has("noderivatives");
    match (non_commercial, share_alike, no_derivatives) {
        (false, false, false) => CC_BY,
        (false, true, _) => CC_BY_SA,
        (false, false, true) => CC_BY_ND,
        (true, false, false) => CC_BY_NC,
        (true, true, _) => CC_BY_NC_SA,
        (true, false, true) => CC_BY_NC_ND,
    }
}

/// Whether a licence asks for the author to be credited.
pub fn needs_credit(code: &str) -> bool {
    !matches!(code, CC0 | PD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn licences_are_read_however_they_are_written() {
        assert_eq!(normalize("CC0 1.0"), CC0);
        assert_eq!(normalize("CC0"), CC0);
        assert_eq!(normalize(CC0_URL), CC0);
        assert_eq!(normalize("Public Domain Mark"), PD);
        assert_eq!(normalize("CC BY 4.0"), CC_BY);
        assert_eq!(
            normalize("Attribution 4.0 International (CC BY 4.0)"),
            CC_BY
        );
        assert_eq!(normalize("CC BY-NC-SA 3.0"), CC_BY_NC_SA);
        assert_eq!(normalize("CC-BY-ND"), CC_BY_ND);
        assert_eq!(
            normalize("https://creativecommons.org/licenses/by-sa/4.0/"),
            CC_BY_SA
        );
        assert_eq!(normalize("CC SAMPLING+ 1.0"), SAMPLING_PLUS);
        assert_eq!(normalize("All rights reserved"), UNKNOWN);
        assert_eq!(normalize(""), UNKNOWN);
    }

    #[test]
    fn openverse_codes_map_both_ways() {
        assert_eq!(from_openverse("by-nc"), CC_BY_NC);
        assert_eq!(from_openverse("pdm"), PD);
        assert_eq!(from_openverse("mystery"), UNKNOWN);
        assert_eq!(to_openverse(CC0), Some("cc0"));
        assert_eq!(to_openverse("cc-by-sa"), Some("by-sa"));
        assert_eq!(to_openverse(UNKNOWN), None);
    }

    #[test]
    fn only_public_domain_needs_no_credit() {
        assert!(!needs_credit(CC0));
        assert!(!needs_credit(PD));
        assert!(needs_credit(CC_BY));
        assert!(needs_credit(UNKNOWN));
    }
}
