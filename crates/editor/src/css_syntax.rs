//! CSS syntax highlighting for stylesheet tabs, in the same colors as Lua's
//! (see `lua_syntax.rs`): selectors like keywords, property names like
//! builtins, and numbers, colors, strings and comments as in Lua.

use std::ops::Range;

use eframe::egui::{self, FontId, text::LayoutJob};

use crate::lua_syntax::{self, TokenKind};

/// Splits `css` into pieces that cover all of it, in order, as byte ranges
/// on character boundaries.
pub fn tokens(css: &str) -> Vec<(TokenKind, Range<usize>)> {
    let bytes = css.as_bytes();
    let mut tokens = Vec::new();
    // Inside `{ }`, and past the `:` of a declaration.
    let (mut in_block, mut in_value) = (false, false);
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let kind = match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = css[i + 2..]
                    .find("*/")
                    .map_or(css.len(), |end| i + 2 + end + 2);
                TokenKind::Comment
            }
            quote @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote && bytes[i] != b'\n' {
                    // A backslash escapes the character after it, which may
                    // be more than one byte.
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += css[i..].chars().next().map_or(1, char::len_utf8);
                }
                i = (i + 1).min(css.len());
                TokenKind::String
            }
            b'{' => {
                (in_block, in_value) = (true, false);
                i += 1;
                TokenKind::Plain
            }
            b'}' => {
                (in_block, in_value) = (false, false);
                i += 1;
                TokenKind::Plain
            }
            b':' if in_block => {
                in_value = true;
                i += 1;
                TokenKind::Plain
            }
            b';' => {
                in_value = false;
                i += 1;
                TokenKind::Plain
            }
            b'#' if in_value => {
                i = word_end(bytes, i + 1);
                TokenKind::Number
            }
            byte if in_value && (byte.is_ascii_digit() || byte == b'.') => {
                i = word_end(bytes, i + 1);
                TokenKind::Number
            }
            byte if is_word(byte) || (!in_block && matches!(byte, b'.' | b'#' | b'*')) => {
                i = word_end(bytes, i + 1);
                match (in_block, in_value) {
                    (false, _) => TokenKind::Keyword,
                    (true, false) => TokenKind::Builtin,
                    (true, true) => TokenKind::Plain,
                }
            }
            _ => {
                // One character, however many bytes it is.
                i += css[i..].chars().next().map_or(1, char::len_utf8);
                TokenKind::Plain
            }
        };
        tokens.push((kind, start..i.min(css.len())));
    }
    tokens
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'%')
}

fn word_end(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && is_word(bytes[i]) {
        i += 1;
    }
    i
}

/// `css`, colored by kind of piece, in `font`, unwrapped.
pub fn layout_job(css: &str, font: FontId, visuals: &egui::Visuals) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (kind, range) in tokens(css) {
        let format = egui::TextFormat::simple(
            font.clone(),
            lua_syntax::color(kind, visuals.dark_mode, visuals.text_color()),
        );
        job.append(&css[range], 0.0, format);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_properties_values_and_comments_are_told_apart() {
        use TokenKind::*;
        let css = "/* hud */ .hud #score { width: 72%; color: #fff; font-family: 'Arial' } ü";
        let colored: Vec<_> = tokens(css)
            .into_iter()
            .filter(|(kind, _)| *kind != Plain)
            .map(|(kind, range)| (kind, &css[range]))
            .collect();
        assert_eq!(
            colored,
            [
                (Comment, "/* hud */"),
                (Keyword, ".hud"),
                (Keyword, "#score"),
                (Builtin, "width"),
                (Number, "72%"),
                (Builtin, "color"),
                (Number, "#fff"),
                (Builtin, "font-family"),
                (String, "'Arial'"),
            ]
        );
        // The pieces cover the whole text, so nothing is lost when drawn.
        assert_eq!(tokens(css).last().unwrap().1.end, css.len());
    }
}
