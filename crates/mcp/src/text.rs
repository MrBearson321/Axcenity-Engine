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

/// `bytes` in standard base64 with padding, as MCP's image content takes
/// them (written here: one function isn't worth a dependency).
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        // RFC 4648's test vectors.
        for (text, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(text.as_bytes()), encoded);
        }
    }

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
