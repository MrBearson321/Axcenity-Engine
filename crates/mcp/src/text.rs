//! Writing results compactly: numbers to 3 decimals with no trailing
//! zeros, and a rough token count for keeping results and tool lists small.

/// `x` with at most 3 decimals and no trailing zeros: `1`, `0.5`, `-2.125`.
/// `-0` reads as `0`.
pub fn number(x: f64) -> String {
    if !x.is_finite() {
        return if x.is_nan() {
            "nan".into()
        } else if x > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let text = format!("{x:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

/// About how many tokens `text` costs a model. Tokenizers differ (Claude's,
/// OpenAI's o200k, local models'), and for the short words, punctuation and
/// numbers of JSON and these results they average a little under 4
/// characters a token, so this counts one per 3.5 characters, rounded up,
/// to err on the side of too many. Budgets compare against this.
pub fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() * 2).div_ceil(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short() {
        assert_eq!(number(1.0), "1");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(-2.1254), "-2.125");
        assert_eq!(number(10.0004), "10");
        assert_eq!(number(-0.0001), "0");
        assert_eq!(number(1234.5), "1234.5");
    }

    #[test]
    fn a_token_estimate_is_one_per_three_and_a_half_characters() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcdefg"), 2);
        assert_eq!(estimate_tokens("abcdefgh"), 3);
    }
}
