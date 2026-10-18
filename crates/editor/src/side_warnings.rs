//! Warnings for using an API on the wrong side, before the game runs: for
//! example `UserInputService` in a Script, which only works in a LocalScript.
//!
//! The check reads the code's words with the syntax highlighter's tokenizer
//! (so comments don't count) and looks each name, and each string such as
//! `"UserInputService"` in `game:GetService("UserInputService")`, up in the
//! script crate's list of side-only APIs. It's a quick check of the text,
//! not of what the code really does, so a local variable that happens to be
//! called `LocalPlayer` in a Script gets a warning too. The real check is
//! the error when the game runs.

use std::collections::HashMap;

use axcenity_core::ClassName;
use axcenity_script::{Side, side_only};

use crate::lua_syntax::{self, TokenKind};

/// One use of an API on the wrong side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideWarning {
    /// Counting from 1.
    pub line: usize,
    /// What's wrong and what to do instead, the same as the error when the game runs.
    pub message: String,
}

/// The wrong-side uses in `source`, the code of a script of `class` (Script
/// or LocalScript), in the order they appear, at most one per API per line.
pub fn side_warnings(source: &str, class: ClassName) -> Vec<SideWarning> {
    let Some(side) = Side::of_class(class) else {
        return Vec::new();
    };
    let mut warnings: Vec<SideWarning> = Vec::new();
    for (kind, range) in lua_syntax::tokens(source) {
        let text = &source[range.clone()];
        let words: Vec<(usize, &str)> = match kind {
            TokenKind::Plain | TokenKind::Builtin => words(text),
            // The inside of a short string; long strings and other
            // pieces never name an API by themselves.
            TokenKind::String => text
                .get(1..text.len().saturating_sub(1))
                .map(|inner| vec![(1, inner)])
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        for (offset, word) in words {
            let Some(api) = side_only(word) else {
                continue;
            };
            if api.side == side {
                continue;
            }
            let start = range.start + offset;
            let line = source[..start].matches('\n').count() + 1;
            let message = api.message();
            let repeated = warnings
                .iter()
                .any(|w| w.line == line && w.message == message);
            if !repeated {
                warnings.push(SideWarning { line, message });
            }
        }
    }
    warnings
}

/// The names in a plain piece of code, with where each starts in it.
fn words(text: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices().chain([(text.len(), ' ')]) {
        let is_name = c.is_ascii_alphanumeric() || c == '_';
        match (is_name, start) {
            (true, None) => start = Some(i),
            (false, Some(from)) => {
                found.push((from, &text[from..i]));
                start = None;
            }
            _ => {}
        }
    }
    found
}

/// The warnings of each script, worked out again only when its code changes.
#[derive(Default)]
pub struct WarningCache {
    /// By script path: the code the warnings are for, and the warnings.
    scripts: HashMap<String, (String, Vec<SideWarning>)>,
}

impl WarningCache {
    /// The warnings for the script at `path` (whose class `path` decides)
    /// with `source` in it.
    pub fn get(&mut self, path: &str, source: &str) -> &[SideWarning] {
        let class = axcenity_project::script_class(path);
        let entry = self
            .scripts
            .entry(path.to_string())
            .or_insert_with(|| (source.to_string(), side_warnings(source, class)));
        if entry.0 != source {
            *entry = (source.to_string(), side_warnings(source, class));
        }
        &entry.1
    }
}

/// The hover text of a warning badge: each warning on its own line.
pub fn badge_text(warnings: &[SideWarning]) -> String {
    warnings
        .iter()
        .map(|w| format!("Line {}: {}", w.line, w.message))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_in_a_script_and_server_events_in_a_local_script_are_warned_about() {
        let source = "-- UserInputService in a comment is fine\n\
                      local uis = game:GetService(\"UserInputService\")\n\
                      local camera = workspace.CurrentCamera\n\
                      remote.OnServerEvent:Connect(print) remote.OnServerEvent:Connect(print)\n";
        let warnings = side_warnings(source, ClassName::Script);
        let lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();
        assert_eq!(lines, [2, 3], "{warnings:?}");
        assert!(
            warnings[0]
                .message
                .starts_with("UserInputService only works in a LocalScript")
        );

        let warnings = side_warnings(source, ClassName::LocalScript);
        // Once for the line, though it's used twice there.
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line, 4);
        assert!(
            warnings[0]
                .message
                .starts_with("OnServerEvent only works in a Script")
        );
    }

    #[test]
    fn names_that_only_contain_an_api_name_are_fine() {
        let source = "local myUserInputService = 1\nlocal LocalPlayers = 2\n";
        assert_eq!(side_warnings(source, ClassName::Script), []);
    }

    #[test]
    fn the_cache_works_warnings_out_again_when_the_code_changes() {
        let mut cache = WarningCache::default();
        let path = "scripts/move.server.lua";
        assert_eq!(cache.get(path, "print(1)"), []);
        assert_eq!(
            cache.get(path, "local p = game.Players.LocalPlayer").len(),
            1
        );
        // A LocalScript may use it.
        assert_eq!(
            cache.get(
                "scripts/me.client.lua",
                "local p = game.Players.LocalPlayer"
            ),
            []
        );
    }
}
