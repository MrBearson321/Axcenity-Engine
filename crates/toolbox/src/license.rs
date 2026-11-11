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

/// What a licence lets a game do with an asset, in the Django Toolbox's
/// terms. Not legal advice: it reads the licence's code, nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    /// The author must be credited.
    pub credit: bool,
    /// It may be used in a game that is sold or earns money.
    pub commercial: bool,
    /// It may be changed (cropped, recoloured, remixed).
    pub modify: bool,
    /// Changed versions must be shared under the same licence.
    pub share_alike: bool,
    /// The code was recognised; an unknown licence allows nothing.
    pub known: bool,
}

/// What `code` allows and asks.
pub fn policy(code: &str) -> Policy {
    let code = normalize(code);
    let known = code != UNKNOWN;
    let non_commercial = matches!(
        code,
        CC_BY_NC | CC_BY_NC_SA | CC_BY_NC_ND | NC_SAMPLING_PLUS
    );
    Policy {
        credit: needs_credit(code),
        commercial: known && !non_commercial,
        modify: known && !matches!(code, CC_BY_ND | CC_BY_NC_ND),
        share_alike: matches!(code, CC_BY_SA | CC_BY_NC_SA),
        known,
    }
}

/// Whether a licence is one the Toolbox shows by default: free for any
/// game, at most asking for a credit.
pub fn is_open(code: &str) -> bool {
    matches!(normalize(code), CC0 | PD | CC_BY)
}

/// How a licence reads on a badge: "CC0 1.0", "CC BY-NC 4.0", "Public
/// domain", "Unknown licence".
pub fn display(code: &str, version: &str) -> String {
    let name = match normalize(code) {
        PD => return "Public domain".to_owned(),
        UNKNOWN => return "Unknown licence".to_owned(),
        CC0 => "CC0".to_owned(),
        SAMPLING_PLUS => "CC Sampling+".to_owned(),
        NC_SAMPLING_PLUS => "CC NC-Sampling+".to_owned(),
        other => other.replacen("CC-", "CC ", 1),
    };
    match version.trim() {
        "" => name,
        version => format!("{name} {version}"),
    }
}

/// What a licence allows and asks, in plain words, for the details drawer.
pub fn plain_words(code: &str) -> String {
    let policy = policy(code);
    if !policy.known {
        return "The licence isn't known, so don't use it in a game until you've checked \
                its source."
            .to_owned();
    }
    let mut words = vec![if policy.credit {
        "Free to use if you credit the author."
    } else {
        "Free to use for anything; no credit needed."
    }];
    if !policy.commercial {
        words.push("Not for a game that is sold or earns money.");
    }
    if !policy.modify {
        words.push("Use it unchanged: no edits or remixes.");
    }
    if policy.share_alike {
        words.push("Changed versions must be shared under the same licence.");
    }
    words.join(" ")
}

/// Why a licence blocks a commercial game, or `None` when it doesn't.
pub fn commercial_block(code: &str) -> Option<&'static str> {
    let policy = policy(code);
    if !policy.known {
        Some("its licence is unknown")
    } else if !policy.commercial {
        Some("its licence is non-commercial")
    } else {
        None
    }
}

/// The credit line for one asset: the source's own text when it gives one,
/// else "\"Name\" by Author, from Source, CC BY 4.0 (url)".
pub fn credit_line(record: &crate::AssetRecord, source_name: &str) -> String {
    if !record.attribution.trim().is_empty() {
        return record.attribution.trim().to_owned();
    }
    let mut line = format!("\"{}\"", record.name);
    let authors: Vec<&str> = record
        .authors
        .iter()
        .map(|author| author.trim())
        .filter(|author| !author.is_empty())
        .collect();
    if !authors.is_empty() {
        line.push_str(&format!(" by {}", authors.join(", ")));
    }
    if !source_name.is_empty() {
        line.push_str(&format!(", from {source_name}"));
    }
    line.push_str(&format!(
        ", {}",
        display(&record.license, &record.license_version)
    ));
    if !record.url.is_empty() {
        line.push_str(&format!(" ({})", record.url));
    }
    line
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

    #[test]
    fn policies_follow_the_licence() {
        let by = policy(CC_BY);
        assert!(by.credit && by.commercial && by.modify && !by.share_alike);
        let nc = policy("CC BY-NC-SA 4.0");
        assert!(!nc.commercial && nc.share_alike && nc.modify);
        assert!(!policy(CC_BY_ND).modify);
        assert!(!policy(CC0).credit);
        let unknown = policy("All rights reserved");
        assert!(!unknown.known && !unknown.commercial && !unknown.modify);
        assert_eq!(
            commercial_block(CC_BY_NC),
            Some("its licence is non-commercial")
        );
        assert_eq!(commercial_block(UNKNOWN), Some("its licence is unknown"));
        assert_eq!(commercial_block(CC_BY), None);
        assert!(is_open(CC0) && is_open(PD) && is_open(CC_BY) && !is_open(CC_BY_SA));
    }

    #[test]
    fn licences_read_plainly() {
        assert_eq!(display(CC0, "1.0"), "CC0 1.0");
        assert_eq!(display(CC_BY_NC, "4.0"), "CC BY-NC 4.0");
        assert_eq!(display(PD, ""), "Public domain");
        assert_eq!(display("nonsense", "2"), "Unknown licence");
        assert!(plain_words(CC0).contains("no credit needed"));
        assert!(plain_words(CC_BY_NC_ND).contains("Not for a game"));
        assert!(plain_words(CC_BY_NC_ND).contains("unchanged"));
    }

    #[test]
    fn credit_lines_name_the_work_author_source_and_licence() {
        let mut record = crate::AssetRecord {
            provider: "polyhaven".into(),
            id: "red_brick_03".into(),
            name: "Red Brick 03".into(),
            url: "https://polyhaven.com/a/red_brick_03".into(),
            authors: vec!["Rob Tuytel".into()],
            license: CC0.into(),
            license_version: "1.0".into(),
            ..Default::default()
        };
        assert_eq!(
            credit_line(&record, "Poly Haven"),
            "\"Red Brick 03\" by Rob Tuytel, from Poly Haven, CC0 1.0 \
             (https://polyhaven.com/a/red_brick_03)"
        );
        record.attribution = "\"Rain\" by Ann, CC BY 4.0".into();
        assert_eq!(
            credit_line(&record, "Openverse"),
            "\"Rain\" by Ann, CC BY 4.0"
        );
    }
}
