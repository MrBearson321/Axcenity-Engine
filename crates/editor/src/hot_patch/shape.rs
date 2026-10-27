//! What changed between two versions of a Rust file, as far as
//! hot-patching cares. `subsecond` swaps in new versions of functions, so a
//! change inside function bodies, or a new or removed item, can be patched.
//! A changed struct, enum, signature, constant or static can't: the running
//! editor's data and its callers were built for the old one.
//!
//! This reads just enough Rust to tell those apart: tokens (comments and
//! whitespace dropped), bracketed groups, and items, looking inside `impl`,
//! `trait` and inline `mod` blocks. It chooses between a patch and a
//! rebuild; it isn't a compiler. Text it can't read (brackets that don't
//! match, halfway through an edit) counts as patchable, and dx reports the
//! error when it builds the patch.

/// A token, or a bracketed group of them.
#[derive(Clone, Debug, PartialEq)]
enum Tree {
    Token(String),
    /// The opening bracket (`(`, `[` or `{`) and what's inside.
    Group(char, Vec<Tree>),
}

/// One item, and the part of it that can't change without a rebuild.
struct Item {
    /// Unique in the file, such as `impl Body / fn step`.
    key: String,
    /// What a message calls it, such as `fn Body::step`.
    name: String,
    /// A function's attributes and signature, or all of any other item.
    shape: String,
    function: bool,
}

/// Why `new` can't be patched over `old`, naming the first item whose
/// signature or definition changed, or `None` when only function bodies
/// changed (or items were added or removed).
pub fn unpatchable(old: &str, new: &str) -> Option<String> {
    let (old, new) = (items_of(old)?, items_of(new)?);
    new.iter().find_map(|item| {
        let before = old.iter().find(|before| before.key == item.key)?;
        (before.shape != item.shape).then(|| {
            if item.function {
                format!("`{}` changed its signature", item.name)
            } else {
                format!("`{}` changed its definition", item.name)
            }
        })
    })
}

/// The items in a file, or `None` if its brackets don't match.
fn items_of(text: &str) -> Option<Vec<Item>> {
    let trees = trees(tokens(text))?;
    let mut found = Vec::new();
    items(&trees, "", "", &mut found);
    Some(found)
}

/// Splits Rust source into tokens: identifiers, literals (whole), lifetimes
/// and single punctuation characters. Comments and whitespace are dropped.
fn tokens(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && next == Some('*') {
            i = block_comment_end(&chars, i);
            continue;
        }
        let start = i;
        if c.is_alphabetic() || c == '_' {
            i = word_end(&chars, i);
            let word: String = chars[start..i].iter().collect();
            match (word.as_str(), chars.get(i)) {
                ("b" | "c", Some('"')) => i = string_end(&chars, i),
                ("b", Some('\'')) => i = char_end(&chars, i),
                ("r" | "br" | "cr", Some('"' | '#')) => i = raw_end(&chars, i),
                _ => {}
            }
        } else if c.is_ascii_digit() {
            i = word_end(&chars, i);
            // A decimal point, but not a range (`0..2`).
            while chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                i = word_end(&chars, i + 1);
            }
        } else if c == '"' {
            i = string_end(&chars, i);
        } else if c == '\'' {
            i = if next == Some('\\') || chars.get(i + 2) == Some(&'\'') {
                char_end(&chars, i)
            } else {
                // A lifetime or a loop label.
                word_end(&chars, i + 1)
            };
        } else {
            i += 1;
        }
        found.push(chars[start..i.min(chars.len())].iter().collect());
    }
    found
}

fn word_end(chars: &[char], mut i: usize) -> usize {
    while chars
        .get(i)
        .is_some_and(|c| c.is_alphanumeric() || *c == '_')
    {
        i += 1;
    }
    i
}

/// After a block comment starting at `i`, which may hold others.
fn block_comment_end(chars: &[char], mut i: usize) -> usize {
    let mut depth = 0;
    while i < chars.len() {
        match (chars[i], chars.get(i + 1)) {
            ('/', Some('*')) => {
                depth += 1;
                i += 2;
            }
            ('*', Some('/')) => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    break;
                }
            }
            _ => i += 1,
        }
    }
    i
}

/// After the string whose opening quote is at `i`.
fn string_end(chars: &[char], mut i: usize) -> usize {
    i += 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

/// After the character literal whose opening quote is at `i`.
fn char_end(chars: &[char], i: usize) -> usize {
    // Past the quote and the first character, escaped or not.
    let mut i = i + if chars.get(i + 1) == Some(&'\\') {
        3
    } else {
        2
    };
    while i < chars.len() && chars[i] != '\'' {
        i += 1;
    }
    i + 1
}

/// After a raw string (`r#"…"#`) whose hashes or quote start at `i`, or
/// after a raw identifier (`r#type`).
fn raw_end(chars: &[char], i: usize) -> usize {
    let hashes = chars[i..].iter().take_while(|c| **c == '#').count();
    let quote = i + hashes;
    if chars.get(quote) != Some(&'"') {
        return word_end(chars, quote);
    }
    let mut j = quote + 1;
    while j < chars.len() {
        if chars[j] == '"'
            && chars[j + 1..]
                .iter()
                .take(hashes)
                .filter(|c| **c == '#')
                .count()
                == hashes
        {
            return j + 1 + hashes;
        }
        j += 1;
    }
    j
}

/// Gathers bracketed tokens into groups, or `None` if they don't match.
fn trees(tokens: Vec<String>) -> Option<Vec<Tree>> {
    let mut open: Vec<(char, Vec<Tree>)> = vec![(' ', Vec::new())];
    for token in tokens {
        match token.as_str() {
            "(" | "[" | "{" => open.push((token.chars().next()?, Vec::new())),
            ")" | "]" | "}" => {
                let (bracket, inside) = open.pop()?;
                let closes = match bracket {
                    '(' => ")",
                    '[' => "]",
                    '{' => "}",
                    _ => return None,
                };
                if closes != token {
                    return None;
                }
                open.last_mut()?.1.push(Tree::Group(bracket, inside));
            }
            _ => open.last_mut()?.1.push(Tree::Token(token)),
        }
    }
    let (_, top) = open.pop()?;
    open.is_empty().then_some(top)
}

/// The trees as text, one space between tokens.
fn text(trees: &[Tree]) -> String {
    let parts: Vec<String> = trees
        .iter()
        .map(|tree| match tree {
            Tree::Token(token) => token.clone(),
            Tree::Group(bracket, inside) => {
                let close = match bracket {
                    '(' => ')',
                    '[' => ']',
                    _ => '}',
                };
                format!("{bracket} {} {close}", text(inside))
            }
        })
        .collect();
    parts.join(" ")
}

fn token(tree: Option<&Tree>) -> Option<&str> {
    match tree {
        Some(Tree::Token(token)) => Some(token),
        _ => None,
    }
}

fn is_identifier(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
}

/// The items in `trees`, added to `found`. `key` and `path` start the keys
/// and names of the items inside a block, such as `impl Body / ` and `Body::`.
fn items(trees: &[Tree], key: &str, path: &str, found: &mut Vec<Item>) {
    let mut i = 0;
    while i < trees.len() {
        let start = i;
        // An inner attribute, `#![…]`, stands alone.
        if token(trees.get(i)) == Some("#") && token(trees.get(i + 1)) == Some("!") {
            let end = (i + 3).min(trees.len());
            add(found, key, "#!", path, text(&trees[i..end]), false);
            i = end;
            continue;
        }
        // Outer attributes, `#[…]`, belong to the item after them.
        while token(trees.get(i)) == Some("#")
            && matches!(trees.get(i + 1), Some(Tree::Group('[', _)))
        {
            i += 2;
        }
        let attributes = text(&trees[start..i]);
        let keyword = skip_modifiers(trees, i);
        let kind = token(trees.get(keyword)).unwrap_or_default().to_owned();
        // These end at `;` even when a `{…}` (a struct literal, a block) comes first.
        let semicolon_only = matches!(kind.as_str(), "const" | "static" | "use" | "type");
        let mut end = keyword;
        while end < trees.len() {
            let ends = match &trees[end] {
                Tree::Token(token) => token == ";",
                Tree::Group(bracket, _) => *bracket == '{' && !semicolon_only,
            };
            end += 1;
            if ends {
                break;
            }
        }
        i = end.max(start + 1);
        let item = &trees[start..i];
        // Tests and imports aren't part of the running editor's data.
        if attributes.contains("cfg ( test )") || kind == "use" || kind.is_empty() {
            continue;
        }
        let name = token(trees.get(keyword + 1)).unwrap_or_default();
        let block = match item.last() {
            Some(Tree::Group('{', inside)) => Some(inside),
            _ => None,
        };
        // Everything but the trailing `{…}` block.
        let head = &item[..item.len() - usize::from(block.is_some())];
        match (kind.as_str(), block) {
            ("fn", _) => add(found, key, &format!("fn {name}"), path, text(head), true),
            ("impl", Some(inside)) => {
                let header = text(&head[keyword - start..]);
                let owner = self_type(&trees[keyword + 1..start + head.len()]);
                items(
                    inside,
                    &format!("{key}{header} / "),
                    &format!("{path}{owner}::"),
                    found,
                );
            }
            ("trait" | "mod", Some(inside)) => {
                let title = format!("{kind} {name}");
                if kind == "trait" {
                    add(found, key, &title, path, text(head), false);
                }
                items(
                    inside,
                    &format!("{key}{title} / "),
                    &format!("{path}{name}::"),
                    found,
                );
            }
            _ => {
                // `name!(…)` is a macro: `thread_local!`, `macro_rules! name`.
                let title = if name == "!" {
                    match token(trees.get(keyword + 2)) {
                        Some(macro_name) if kind == "macro_rules" => {
                            format!("macro_rules! {macro_name}")
                        }
                        _ => format!("{kind}!"),
                    }
                } else {
                    format!("{kind} {name}")
                };
                add(found, key, &title, path, text(item), false);
            }
        }
    }
}

/// Past `pub`, `pub(crate)`, `unsafe`, `async`, `const fn` and
/// `extern "C" fn` to the token that says what the item is.
fn skip_modifiers(trees: &[Tree], mut i: usize) -> usize {
    loop {
        let next = token(trees.get(i + 1));
        match token(trees.get(i)) {
            Some("pub") => {
                i += 1;
                if matches!(trees.get(i), Some(Tree::Group('(', _))) {
                    i += 1;
                }
            }
            Some("unsafe" | "async" | "default" | "safe") => i += 1,
            Some("const") if matches!(next, Some("fn" | "unsafe" | "async" | "extern")) => i += 1,
            Some("extern") if next.is_some_and(|abi| abi.starts_with('"')) => i += 2,
            Some("extern") if next == Some("fn") => i += 1,
            _ => return i,
        }
    }
}

/// The type an `impl` header is for (after its generics and `for`), such as
/// `Body` in `impl<T> From<T> for crate::Body<T>`.
fn self_type(header: &[Tree]) -> String {
    let tokens: Vec<&str> = header.iter().filter_map(|tree| token(Some(tree))).collect();
    // Past the generics: count angle brackets, leaving out `->`'s.
    let mut i = 0;
    if tokens.first() == Some(&"<") {
        let mut depth = 0;
        while i < tokens.len() {
            match tokens[i] {
                "<" => depth += 1,
                ">" if i == 0 || tokens[i - 1] != "-" => depth -= 1,
                _ => {}
            }
            i += 1;
            if depth == 0 {
                break;
            }
        }
    }
    let rest = &tokens[i..];
    let rest = match rest.iter().position(|token| *token == "for") {
        Some(at) => &rest[at + 1..],
        None => rest,
    };
    // The last path segment before the type's own generics or a `where`.
    rest.iter()
        .take_while(|token| **token != "<" && **token != "where")
        .filter(|token| is_identifier(token) && !matches!(**token, "dyn" | "mut"))
        .last()
        .map_or_else(String::new, |name| (*name).to_owned())
}

/// Adds an item, numbering keys that repeat (several `const _`, two
/// `thread_local!` blocks) in the order they come.
fn add(found: &mut Vec<Item>, key: &str, title: &str, path: &str, shape: String, function: bool) {
    let mut unique = format!("{key}{title}");
    let base = unique.clone();
    let mut count = 1;
    while found.iter().any(|item| item.key == unique) {
        count += 1;
        unique = format!("{base}#{count}");
    }
    let name = match title.split_once(' ') {
        Some((kind, name)) if !title.contains('!') => format!("{kind} {path}{name}"),
        _ => title.to_owned(),
    };
    found.push(Item {
        key: unique,
        name,
        shape,
        function,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"
use std::fmt;

/// A physics body.
#[derive(Clone, Debug)]
pub struct Body {
    pub position: f32,
    velocity: f32,
}

const GRAVITY: f32 = -9.81;

impl Body {
    /// One step.
    pub fn step(&mut self, seconds: f32) {
        self.velocity += GRAVITY * seconds;
        self.position += self.velocity * seconds;
    }
}

impl fmt::Display for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.position)
    }
}
"#;

    fn changed(from: &str, to: &str) -> String {
        BODY.replacen(from, to, 1)
    }

    #[test]
    fn a_changed_function_body_is_patchable() {
        let faster = changed("GRAVITY * seconds;", "GRAVITY * seconds * 2.0;");
        assert_eq!(unpatchable(BODY, &faster), None);
        // Comments, doc comments, spacing and imports don't matter either.
        let tidied = changed("/// One step.", "// Moves it.")
            .replace("    velocity: f32,", "velocity:f32,")
            .replace("use std::fmt;", "use std::fmt::{self, Write};");
        assert_eq!(unpatchable(BODY, &tidied), None);
    }

    #[test]
    fn a_struct_with_other_fields_needs_a_rebuild() {
        let more = changed("    velocity: f32,", "    velocity: f32,\n    mass: f32,");
        assert_eq!(
            unpatchable(BODY, &more).as_deref(),
            Some("`struct Body` changed its definition")
        );
        let derived = changed(
            "#[derive(Clone, Debug)]",
            "#[derive(Clone, Debug, Default)]",
        );
        assert!(unpatchable(BODY, &derived).is_some());
    }

    #[test]
    fn a_changed_signature_constant_or_trait_needs_a_rebuild() {
        let signature = changed("seconds: f32)", "seconds: f64)");
        assert_eq!(
            unpatchable(BODY, &signature).as_deref(),
            Some("`fn Body::step` changed its signature")
        );
        let constant = changed("-9.81", "-1.62");
        assert_eq!(
            unpatchable(BODY, &constant).as_deref(),
            Some("`const GRAVITY` changed its definition")
        );
        let traits = "pub trait Step {\n    fn step(&mut self);\n}\n";
        let other = traits.replace("(&mut self)", "(&mut self, seconds: f32)");
        assert_eq!(
            unpatchable(traits, &other).as_deref(),
            Some("`fn Step::step` changed its signature")
        );
    }

    #[test]
    fn new_and_removed_items_are_patchable() {
        let added = format!("{BODY}\nfn helper() -> u32 {{ 1 }}\nstruct Extra;\n");
        assert_eq!(unpatchable(BODY, &added), None);
        assert_eq!(unpatchable(&added, BODY), None);
        assert_eq!(unpatchable("", BODY), None);
        let method = changed(
            "impl Body {",
            "impl Body {\n    pub fn stop(&mut self) { self.velocity = 0.0; }",
        );
        assert_eq!(unpatchable(BODY, &method), None);
    }

    #[test]
    fn methods_of_two_impls_are_told_apart() {
        let display = changed("&mut fmt::Formatter<'_>", "&mut fmt::Formatter");
        assert_eq!(
            unpatchable(BODY, &display).as_deref(),
            Some("`fn Body::fmt` changed its signature")
        );
        assert_eq!(
            self_type(&trees(tokens("<T: Fn() -> u8> From<T> for crate::Body<T>")).unwrap()),
            "Body"
        );
        assert_eq!(self_type(&trees(tokens("dyn Step")).unwrap()), "Step");
    }

    #[test]
    fn tests_and_text_that_looks_like_brackets_are_left_alone() {
        let with_tests =
            format!("{BODY}\n#[cfg(test)]\nmod tests {{\n    struct Fake {{ a: u8 }}\n}}\n");
        let other_tests = with_tests.replace("a: u8", "a: u8, b: u8");
        assert_eq!(unpatchable(&with_tests, &other_tests), None);
        // Brackets in strings, characters, raw strings and comments don't count.
        let tricky = r##"
fn f<'a>(x: &'a str) -> char {
    let _ = "}{"; let _ = r#"" { "#; let _ = '{'; let _ = b'}'; /* { /* } */ */
    let _ = 0..2; let _ = 1.5e3; let r#type = 1;
    'outer: loop { break 'outer; }
    '}'
}
"##;
        assert!(items_of(tricky).is_some());
        let body_only = tricky.replace("1.5e3", "2.5e3");
        assert_eq!(unpatchable(tricky, &body_only), None);
        let signature = tricky.replace("-> char", "-> u8");
        assert_eq!(
            unpatchable(tricky, &signature).as_deref(),
            Some("`fn f` changed its signature")
        );
    }

    #[test]
    fn every_engine_source_file_reads() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let files = crate::rebuild::engine_files(&workspace);
        let rust: Vec<&String> = files.iter().filter(|path| path.ends_with(".rs")).collect();
        assert!(rust.len() > 100, "{}", rust.len());
        for path in rust {
            let text = std::fs::read_to_string(workspace.join(path)).unwrap();
            let items = items_of(&text).unwrap_or_else(|| panic!("{path} didn't read"));
            assert!(!items.is_empty() || text.trim().is_empty(), "{path}");
        }
    }

    #[test]
    fn half_written_code_is_left_to_dx() {
        assert_eq!(unpatchable(BODY, "struct Body {"), None);
        assert!(items_of("fn f() { (] }").is_none());
    }

    #[test]
    fn macros_and_statics_are_definitions() {
        let source = "thread_local! { static A: u8 = 1; }\nstatic B: [u8; 2] = [1, 2];\n";
        assert_eq!(
            unpatchable(source, &source.replace("= 1;", "= 2;")).as_deref(),
            Some("`thread_local!` changed its definition")
        );
        assert_eq!(
            unpatchable(source, &source.replace("[1, 2]", "[3, 4]")).as_deref(),
            Some("`static B` changed its definition")
        );
    }
}
