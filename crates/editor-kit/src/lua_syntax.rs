//! Lua syntax highlighting for the script editor.
//!
//! [`tokens`] splits Lua code into pieces (keywords, strings, comments and
//! so on) without any UI code, so it's easy to test. [`layout_job`] colors
//! those pieces for egui.

use std::ops::Range;

use eframe::egui::{self, Color32, FontId, text::LayoutJob};

/// What a piece of Lua code is, which decides its color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// Names, operators and spaces.
    Plain,
    Keyword,
    /// `true`, `false` and `nil`.
    Literal,
    /// The engine's globals and Lua's standard functions, such as `workspace` and `print`.
    Builtin,
    Number,
    String,
    Comment,
}

const KEYWORDS: [&str; 19] = [
    "and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in", "local",
    "not", "or", "repeat", "return", "then", "until", "while",
];
const LITERALS: [&str; 3] = ["true", "false", "nil"];
/// The globals a script can use (see `crates/script/src/sandbox.rs` and `api.rs`).
const BUILTINS: [&str; 34] = [
    "game",
    "workspace",
    "script",
    "print",
    "Instance",
    "Vector3",
    "Color3",
    "Enum",
    "task",
    "math",
    "string",
    "table",
    "coroutine",
    "utf8",
    "os",
    "assert",
    "error",
    "pcall",
    "xpcall",
    "type",
    "tostring",
    "tonumber",
    "pairs",
    "ipairs",
    "next",
    "select",
    "rawget",
    "rawset",
    "rawequal",
    "rawlen",
    "setmetatable",
    "getmetatable",
    "_G",
    "_VERSION",
];

/// Splits `code` into pieces that cover all of it, in order, as byte ranges.
/// Every range starts and ends on a character boundary, so slicing `code`
/// with it never panics.
pub fn tokens(code: &str) -> Vec<(TokenKind, Range<usize>)> {
    let bytes = code.as_bytes();
    let mut tokens: Vec<(TokenKind, Range<usize>)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let byte = bytes[i];
        let kind = if bytes[i..].starts_with(b"--") {
            i += 2;
            match long_bracket_level(bytes, i) {
                Some(level) => i = long_bracket_end(bytes, i, level),
                None => i = line_end(bytes, i),
            }
            TokenKind::Comment
        } else if byte == b'"' || byte == b'\'' {
            i = quoted_string_end(bytes, i);
            TokenKind::String
        } else if let Some(level) = long_bracket_level(bytes, i) {
            i = long_bracket_end(bytes, i, level);
            TokenKind::String
        } else if byte.is_ascii_digit()
            || (byte == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            i = number_end(bytes, i);
            TokenKind::Number
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &code[start..i];
            // After `.` or `:` a name is a field or method (`part.Color`,
            // `x.print`), not the global with that name.
            let field = start > 0 && matches!(bytes[start - 1], b'.' | b':');
            if KEYWORDS.contains(&word) {
                TokenKind::Keyword
            } else if LITERALS.contains(&word) {
                TokenKind::Literal
            } else if BUILTINS.contains(&word) && !field {
                TokenKind::Builtin
            } else {
                TokenKind::Plain
            }
        } else {
            // One ASCII character, or a whole non-ASCII one: its UTF-8 bytes
            // are all 0x80 or more, so stop at the next ASCII byte.
            i += 1;
            while i < bytes.len() && bytes[i] >= 0x80 {
                i += 1;
            }
            TokenKind::Plain
        };
        // Joins plain pieces next to each other, so there are fewer of them.
        match tokens.last_mut() {
            Some((TokenKind::Plain, range)) if kind == TokenKind::Plain => range.end = i,
            _ => tokens.push((kind, start..i)),
        }
    }
    tokens
}

/// If a long bracket (`[[`, `[=[`, `[==[`…) starts at `i`, how many `=` it has.
fn long_bracket_level(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'[') {
        return None;
    }
    let equals = bytes[i + 1..].iter().take_while(|&&b| b == b'=').count();
    (bytes.get(i + 1 + equals) == Some(&b'[')).then_some(equals)
}

/// Where the long bracket opened at `i` ends: after its matching `]]` (with
/// the same number of `=`), or at the end of the code if it's never closed.
fn long_bracket_end(bytes: &[u8], i: usize, level: usize) -> usize {
    let mut close = vec![b']'];
    close.extend(std::iter::repeat_n(b'=', level));
    close.push(b']');
    let body = i + level + 2;
    bytes[body..]
        .windows(close.len())
        .position(|window| window == close.as_slice())
        .map_or(bytes.len(), |at| body + at + close.len())
}

/// Where the line containing `i` ends (before the newline).
fn line_end(bytes: &[u8], i: usize) -> usize {
    bytes[i..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |at| i + at)
}

/// Where the string opened by the quote at `i` ends: after the closing
/// quote, or at the end of the line if it's never closed (Lua strings can't
/// span lines without a `\`).
fn quoted_string_end(bytes: &[u8], i: usize) -> usize {
    let quote = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() {
        match bytes[j] {
            // Skips the escaped character, unless it's non-ASCII: then the
            // loop carries on through its bytes, which are never quotes.
            b'\\' if bytes.get(j + 1).is_some_and(u8::is_ascii) => j += 2,
            b'\n' => return j,
            b if b == quote => return j + 1,
            _ => j += 1,
        }
    }
    bytes.len()
}

/// Where the number starting at `i` ends. Covers `12`, `3.5`, `.5`, `1e-3`,
/// `0xFF` and `0x1p4`.
fn number_end(bytes: &[u8], i: usize) -> usize {
    let hex = bytes[i..].starts_with(b"0x") || bytes[i..].starts_with(b"0X");
    let exponent: &[u8] = if hex { b"pP" } else { b"eE" };
    let mut j = i;
    while j < bytes.len() {
        let b = bytes[j];
        let sign_after_exponent =
            matches!(b, b'+' | b'-') && j > i && exponent.contains(&bytes[j - 1]);
        if b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || sign_after_exponent {
            j += 1;
        } else {
            break;
        }
    }
    j
}

/// The color of each kind of piece, in the dark or light theme.
pub fn color(kind: TokenKind, dark: bool, plain: Color32) -> Color32 {
    let rgb = Color32::from_rgb;
    match (kind, dark) {
        (TokenKind::Plain, _) => plain,
        (TokenKind::Keyword, true) => rgb(197, 134, 192),
        (TokenKind::Keyword, false) => rgb(175, 0, 219),
        (TokenKind::Literal, true) => rgb(86, 156, 214),
        (TokenKind::Literal, false) => rgb(0, 0, 255),
        (TokenKind::Builtin, true) => rgb(78, 201, 176),
        (TokenKind::Builtin, false) => rgb(38, 127, 153),
        (TokenKind::Number, true) => rgb(181, 206, 168),
        (TokenKind::Number, false) => rgb(9, 134, 88),
        (TokenKind::String, true) => rgb(206, 145, 120),
        (TokenKind::String, false) => rgb(163, 21, 21),
        (TokenKind::Comment, true) => rgb(106, 153, 85),
        (TokenKind::Comment, false) => rgb(0, 128, 0),
    }
}

/// `code`, colored by kind of piece, in `font`, with long lines left
/// unwrapped (the editor scrolls sideways instead, so line numbers line up).
pub fn layout_job(code: &str, font: FontId, visuals: &egui::Visuals) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (kind, range) in tokens(code) {
        let format = egui::TextFormat::simple(
            font.clone(),
            color(kind, visuals.dark_mode, visuals.text_color()),
        );
        job.append(&code[range], 0.0, format);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pieces of `code` that aren't plain, as (kind, text).
    fn colored(code: &str) -> Vec<(TokenKind, &str)> {
        tokens(code)
            .into_iter()
            .filter(|(kind, _)| *kind != TokenKind::Plain)
            .map(|(kind, range)| (kind, &code[range]))
            .collect()
    }

    #[test]
    fn keywords_strings_numbers_and_comments_are_told_apart() {
        use TokenKind::*;
        let code = "local pad = workspace.Pad -- the pad\nif pad.Anchored == false then print(\"hi\", 'x', 1.5e-3, 0xFF) end";
        assert_eq!(
            colored(code),
            [
                (Keyword, "local"),
                (Builtin, "workspace"),
                (Comment, "-- the pad"),
                (Keyword, "if"),
                (Literal, "false"),
                (Keyword, "then"),
                (Builtin, "print"),
                (String, "\"hi\""),
                (String, "'x'"),
                (Number, "1.5e-3"),
                (Number, "0xFF"),
                (Keyword, "end"),
            ]
        );
    }

    #[test]
    fn long_strings_and_comments_span_lines() {
        use TokenKind::*;
        let code = "--[[ one\ntwo ]] x = [==[ a ]] b ]==] y = 'it\\'s' --[=[ never closed";
        assert_eq!(
            colored(code),
            [
                (Comment, "--[[ one\ntwo ]]"),
                (String, "[==[ a ]] b ]==]"),
                (String, "'it\\'s'"),
                (Comment, "--[=[ never closed"),
            ]
        );
        // An unclosed quote stops at the end of its line.
        assert_eq!(
            colored("s = \"open\nend"),
            [(String, "\"open"), (Keyword, "end")]
        );
    }

    #[test]
    fn names_after_a_dot_or_colon_are_not_globals() {
        assert_eq!(
            colored("x.print(game:workspace())"),
            [(TokenKind::Builtin, "game")]
        );
        // Part of a longer name isn't a keyword either.
        assert!(colored("endless = 1 notable = 2 local_x = 3").len() == 3);
    }

    #[test]
    fn pieces_cover_all_the_code_and_split_only_between_characters() {
        let code = "-- ünïcode ✓\nlocal é = \"日本\\é\" ±1 [[ß]] 'x\\";
        let pieces = tokens(code);
        let mut end = 0;
        for (_, range) in &pieces {
            assert_eq!(range.start, end);
            assert!(code.is_char_boundary(range.start) && code.is_char_boundary(range.end));
            end = range.end;
        }
        assert_eq!(end, code.len());
    }
}
