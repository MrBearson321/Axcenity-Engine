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

/// Every colour written in a value (`#rgb`, `#rrggbb`, `#rrggbbaa`,
/// `rgb(…)`, `rgba(…)` or a name such as `white`), as its byte range and
/// colour, for the editor's swatches.
pub fn colors(css: &str) -> Vec<(Range<usize>, egui::Color32)> {
    let tokens = tokens(css);
    let mut found = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let (kind, range) = (tokens[i].0, tokens[i].1.clone());
        i += 1;
        let text = &css[range.clone()];
        // A `#` in a value is a Number; in a selector it's an Id.
        let whole = if kind == TokenKind::Number && text.starts_with('#') {
            Some(range)
        } else if kind == TokenKind::Plain && text.bytes().all(|b| b.is_ascii_alphabetic()) {
            // A word in a value: a function's name or a colour's.
            let function = text.eq_ignore_ascii_case("rgb") || text.eq_ignore_ascii_case("rgba");
            if function && css[range.end..].starts_with('(') {
                css[range.end..]
                    .find(')')
                    .filter(|&close| !css[range.end..range.end + close].contains(['\n', ';', '}']))
                    .map(|close| range.start..range.end + close + 1)
            } else {
                Some(range)
            }
        } else {
            None
        };
        let Some(whole) = whole else {
            continue;
        };
        let Some((color, alpha)) = axcenity_ui::parse_css_color(&css[whole.clone()]) else {
            continue;
        };
        let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
        found.push((
            whole.clone(),
            egui::Color32::from_rgba_unmultiplied(
                byte(color.r),
                byte(color.g),
                byte(color.b),
                byte(alpha),
            ),
        ));
        // Skip the tokens inside `rgb(…)`.
        while i < tokens.len() && tokens[i].1.start < whole.end {
            i += 1;
        }
    }
    found
}

/// A colour as CSS writes it: `#rrggbb`, or `rgba()` when it's see-through.
pub fn color_text(color: egui::Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r}, {g}, {b}, {:.2})", f32::from(a) / 255.0)
    }
}

/// `css`, colored by kind of piece, in `font`, unwrapped. Each colour
/// starts `swatch` points further on, leaving room for its swatch.
pub fn layout_job(css: &str, font: FontId, visuals: &egui::Visuals, swatch: f32) -> LayoutJob {
    let starts: Vec<usize> = if swatch > 0.0 {
        colors(css)
            .into_iter()
            .map(|(range, _)| range.start)
            .collect()
    } else {
        Vec::new()
    };
    let mut job = LayoutJob::default();
    for (kind, range) in tokens(css) {
        let format = egui::TextFormat::simple(
            font.clone(),
            lua_syntax::color(kind, visuals.dark_mode, visuals.text_color()),
        );
        let leading = if starts.contains(&range.start) {
            swatch
        } else {
            0.0
        };
        job.append(&css[range], leading, format);
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

    #[test]
    fn colors_in_values_are_found_with_their_ranges_but_not_ids() {
        let css = "#fff, .a { color: #ff000080; background-color: rgba(0, 0, 255, 0.5); \
                   border: 2px solid white; font-family: Arial }";
        let found: Vec<_> = colors(css)
            .into_iter()
            .map(|(range, color)| (&css[range], color.to_srgba_unmultiplied()))
            .collect();
        assert_eq!(
            found,
            [
                ("#ff000080", [255, 0, 0, 128]),
                ("rgba(0, 0, 255, 0.5)", [0, 0, 255, 128]),
                ("white", [255, 255, 255, 255]),
            ]
        );
        assert_eq!(color_text(egui::Color32::from_rgb(255, 0, 16)), "#ff0010");
        assert_eq!(
            color_text(egui::Color32::from_rgba_unmultiplied(0, 0, 255, 128)),
            "rgba(0, 0, 255, 0.50)"
        );
    }
}
