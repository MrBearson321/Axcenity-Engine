//! Find in Scripts (Ctrl+Shift+F, unified Phase 70): searches every Lua
//! script and block script, with match case, whole word and regex, lists
//! each match, and replaces them all. This module is the search itself and
//! the window; the app gathers the scripts, goes to a result and makes
//! Replace All one undo step (`app/find_in_scripts.rs`).

use axcenity_blocks::BlockScript;
use eframe::egui;
use regex::{NoExpand, Regex};

use crate::theme::{self, icons};
use crate::widgets;

/// How the query is matched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindOptions {
    pub match_case: bool,
    pub whole_word: bool,
    pub regex: bool,
}

/// A query made into one regular expression, whatever the options.
pub struct Matcher {
    regex: Regex,
    /// A plain query's replacement is used as it is; a regex's may name
    /// its groups (`$1`).
    expand: bool,
}

impl Matcher {
    /// The matcher for `query`, or why it isn't one (a bad regex).
    pub fn new(query: &str, options: FindOptions) -> Result<Matcher, String> {
        if query.is_empty() {
            return Err("type something to find".to_owned());
        }
        let mut pattern = if options.regex {
            query.to_owned()
        } else {
            regex::escape(query)
        };
        if options.whole_word {
            pattern = format!(r"\b(?:{pattern})\b");
        }
        if !options.match_case {
            pattern = format!("(?i){pattern}");
        }
        let regex = Regex::new(&pattern).map_err(|err| match err {
            regex::Error::Syntax(text) => text.lines().last().unwrap_or("bad regex").to_owned(),
            other => other.to_string(),
        })?;
        Ok(Matcher {
            regex,
            expand: options.regex,
        })
    }

    /// `text` with every match replaced, and how many there were.
    pub fn replace(&self, text: &str, replacement: &str) -> (String, usize) {
        let count = self.regex.find_iter(text).count();
        if count == 0 {
            return (text.to_owned(), 0);
        }
        let replaced = if self.expand {
            self.regex.replace_all(text, replacement)
        } else {
            self.regex.replace_all(text, NoExpand(replacement))
        };
        (replaced.into_owned(), count)
    }

    fn is_match(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }
}

/// Where a match is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spot {
    /// A line of a Lua script (from 1), and the match's byte range in it.
    Line {
        line: usize,
        start: usize,
        end: usize,
    },
    /// A block of a block script (`None`: a value not in a block).
    Block(Option<u64>),
}

/// One match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    pub spot: Spot,
    /// The line, or the block's text that matched, to show.
    pub preview: String,
}

/// Every match in the Lua script at `path`.
pub fn search_code(path: &str, text: &str, matcher: &Matcher) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for found in matcher.regex.find_iter(line) {
            hits.push(Hit {
                path: path.to_owned(),
                spot: Spot::Line {
                    line: index + 1,
                    start: found.start(),
                    end: found.end(),
                },
                preview: line.to_owned(),
            });
        }
    }
    hits
}

/// Every block of the block script at `path` with a match in its text
/// (names, paths, messages, variables): one hit per text that matched.
pub fn search_blocks(path: &str, script: &BlockScript, matcher: &Matcher) -> Vec<Hit> {
    let Ok(value) = serde_json::to_value(script) else {
        return Vec::new();
    };
    let mut hits = Vec::new();
    texts(&value, None, &mut |text, block| {
        if matcher.is_match(text) {
            hits.push(Hit {
                path: path.to_owned(),
                spot: Spot::Block(block),
                preview: text.to_owned(),
            });
        }
    });
    hits
}

/// `script` with every match in its texts replaced, and how many there
/// were. `Err` if the result isn't a block script any more.
pub fn replace_blocks(
    script: &BlockScript,
    matcher: &Matcher,
    replacement: &str,
) -> Result<(BlockScript, usize), String> {
    let mut value = serde_json::to_value(script).map_err(|err| err.to_string())?;
    let mut count = 0;
    replace_texts(&mut value, &mut |text| {
        let (replaced, found) = matcher.replace(text, replacement);
        count += found;
        replaced
    });
    let script = serde_json::from_value(value).map_err(|err| err.to_string())?;
    Ok((script, count))
}

/// Calls `found` with each text of a block script's JSON (not a block's
/// `type`) and the id of the block it's in.
fn texts(value: &serde_json::Value, block: Option<u64>, found: &mut dyn FnMut(&str, Option<u64>)) {
    match value {
        serde_json::Value::Object(map) => {
            let block = map.get("id").and_then(serde_json::Value::as_u64).or(block);
            for (key, item) in map {
                match item {
                    serde_json::Value::String(_) if key == "type" => {}
                    serde_json::Value::String(text) => found(text, block),
                    _ => texts(item, block, found),
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                texts(item, block, found);
            }
        }
        serde_json::Value::String(text) => found(text, block),
        _ => {}
    }
}

fn replace_texts(value: &mut serde_json::Value, change: &mut dyn FnMut(&str) -> String) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                match item {
                    serde_json::Value::String(_) if key == "type" => {}
                    serde_json::Value::String(text) => *text = change(text),
                    _ => replace_texts(item, change),
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                replace_texts(item, change);
            }
        }
        serde_json::Value::String(text) => *text = change(text),
        _ => {}
    }
}

/// The Find in Scripts window.
pub struct FindWindow {
    pub open: bool,
    pub query: String,
    pub replacement: String,
    pub options: FindOptions,
    /// The last search's matches, or why it couldn't search.
    pub results: Result<Vec<Hit>, String>,
    /// Put the cursor in the query box in the next frame.
    focus: bool,
}

impl Default for FindWindow {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            replacement: String::new(),
            options: FindOptions::default(),
            results: Err(String::new()),
            focus: false,
        }
    }
}

impl std::fmt::Debug for FindWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FindWindow")
            .field("open", &self.open)
            .finish()
    }
}

/// What the window asks the app to do.
#[derive(Debug, PartialEq)]
pub enum FindAction {
    Search,
    GoTo(Hit),
    ReplaceAll,
}

impl FindWindow {
    pub fn show_and_focus(&mut self) {
        self.open = true;
        self.focus = true;
    }

    /// Draws the window. `can_replace` is false during Play.
    pub fn ui(&mut self, ctx: &egui::Context, can_replace: bool) -> Option<FindAction> {
        if !self.open {
            return None;
        }
        let mut action = None;
        let mut open = self.open;
        let palette = theme::palette(ctx);
        egui::Window::new("Find in Scripts")
            .open(&mut open)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
                let before = (self.query.clone(), self.options);
                ui.horizontal(|ui| {
                    let field = widgets::search_field(ui, &mut self.query, "Find", 220.0);
                    if std::mem::take(&mut self.focus) {
                        field.request_focus();
                    }
                    let toggle = |ui: &mut egui::Ui, on: &mut bool, text: &str, hint: &str| {
                        if ui.selectable_label(*on, text).on_hover_text(hint).clicked() {
                            *on = !*on;
                        }
                    };
                    toggle(ui, &mut self.options.match_case, "Aa", "Match case");
                    toggle(ui, &mut self.options.whole_word, "ab", "Whole word");
                    toggle(ui, &mut self.options.regex, ".*", "Regular expression");
                });
                if (self.query.clone(), self.options) != before {
                    action = Some(FindAction::Search);
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.replacement)
                            .hint_text("Replace with")
                            .desired_width(220.0 + widgets::SEARCH_FIELD_MARGINS),
                    );
                    let any = self.results.as_ref().is_ok_and(|hits| !hits.is_empty());
                    let replace =
                        egui::Button::new(theme::with_icon(icons::ARROWS_CLOCKWISE, "Replace All"));
                    if ui
                        .add_enabled(any && can_replace, replace)
                        .on_hover_text("Replace every match, in every script, as one undo step.")
                        .on_disabled_hover_text(if can_replace {
                            "Nothing to replace."
                        } else {
                            "Press Stop to replace."
                        })
                        .clicked()
                    {
                        action = Some(FindAction::ReplaceAll);
                    }
                });
                ui.separator();
                match &self.results {
                    Err(message) if !self.query.is_empty() => {
                        ui.colored_label(palette.error, message);
                    }
                    Err(_) => {
                        ui.weak("Searches every script and block script.");
                    }
                    Ok(hits) => {
                        let files = {
                            let mut paths: Vec<&str> =
                                hits.iter().map(|h| h.path.as_str()).collect();
                            paths.dedup();
                            paths.len()
                        };
                        ui.weak(format!(
                            "{} {} in {} {}",
                            hits.len(),
                            if hits.len() == 1 { "match" } else { "matches" },
                            files,
                            if files == 1 { "script" } else { "scripts" },
                        ));
                        egui::ScrollArea::vertical()
                            .max_height(360.0)
                            .show(ui, |ui| {
                                let mut last = None;
                                for hit in hits {
                                    if last != Some(&hit.path) {
                                        ui.label(egui::RichText::new(&hit.path).strong());
                                        last = Some(&hit.path);
                                    }
                                    let place = match hit.spot {
                                        Spot::Line { line, .. } => format!("{line}"),
                                        Spot::Block(_) => icons::PUZZLE_PIECE.to_owned(),
                                    };
                                    let text = format!("{place:>5}  {}", hit.preview.trim());
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new(text).monospace(),
                                            )
                                            .frame(false)
                                            .truncate(),
                                        )
                                        .clicked()
                                    {
                                        action = Some(FindAction::GoTo(hit.clone()));
                                    }
                                }
                            });
                    }
                }
            });
        self.open = open;
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher(query: &str, match_case: bool, whole_word: bool, regex: bool) -> Matcher {
        Matcher::new(
            query,
            FindOptions {
                match_case,
                whole_word,
                regex,
            },
        )
        .unwrap()
    }

    #[test]
    fn the_options_change_what_matches() {
        let text = "Pad.Touched\nlocal pad = workspace.Pad\nPadding = 2\n";
        let lines = |m: &Matcher| -> Vec<usize> {
            search_code("a.lua", text, m)
                .into_iter()
                .map(|hit| match hit.spot {
                    Spot::Line { line, .. } => line,
                    Spot::Block(_) => 0,
                })
                .collect()
        };
        assert_eq!(lines(&matcher("pad", false, false, false)), [1, 2, 2, 3]);
        assert_eq!(lines(&matcher("Pad", true, false, false)), [1, 2, 3]);
        assert_eq!(lines(&matcher("Pad", true, true, false)), [1, 2]);
        assert_eq!(lines(&matcher(r"Pad\.\w+", true, false, true)), [1]);
        // A plain query's dots are dots.
        assert!(lines(&matcher("P.d", false, false, false)).is_empty());
    }

    #[test]
    fn a_bad_regex_says_why() {
        let options = FindOptions {
            regex: true,
            ..FindOptions::default()
        };
        assert!(Matcher::new("(", options).is_err());
        assert!(Matcher::new("", FindOptions::default()).is_err());
    }

    #[test]
    fn regex_replacements_use_groups_and_plain_ones_do_not() {
        let (text, count) =
            matcher(r"(\w+)\.Size", true, false, true).replace("a.Size b.Size", "$1.Scale");
        assert_eq!((text.as_str(), count), ("a.Scale b.Scale", 2));
        let (text, _) = matcher("x", true, false, false).replace("x", "$1");
        assert_eq!(text, "$1");
    }
}
