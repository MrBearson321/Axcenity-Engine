//! The Console's command line, under the log (`output.rs`).
//!
//! What's typed is either a built-in command ([`BUILT_INS`], such as
//! `select Workspace.Baseplate`) or Lua, which the app runs against the
//! world being edited, or the game during Play (see `app/console.rs`).
//!
//! Enter runs the line and Shift+Enter starts a new one. Up and Down walk
//! the history (saved between runs) while the input is one line. Tab
//! completes built-ins, global names, and Instance paths, children and
//! properties (`workspace.Base` → `workspace.Baseplate`).

use axcenity_core::{InstanceId, World};
use eframe::egui;
use egui::text::{CCursor, CCursorRange};

use crate::theme::{self, icons};

/// The built-in commands, with what `help` says about each.
pub const BUILT_INS: [(&str, &str); 9] = [
    ("help", "help — list these commands"),
    ("clear", "clear — empty the log"),
    (
        "select",
        "select <name|path> — select an Instance, such as `select Baseplate` or \
         `select Workspace.Model.Door`",
    ),
    (
        "find",
        "find <name> — list every Instance whose name contains <name>, and select them",
    ),
    (
        "open",
        "open <script> — open a script by name or path, such as `open Spinner`",
    ),
    ("play", "play — start Play"),
    ("stop", "stop — stop Play"),
    ("save", "save — save the project"),
    (
        "check",
        "check — scan the project for missing files, invalid references, \
         import failures, and broken dependencies",
    ),
];

/// The most commands the history keeps.
const HISTORY_LIMIT: usize = 200;

/// One line typed into the Console, sorted out.
#[derive(Debug, PartialEq, Eq)]
pub enum Command<'a> {
    Help,
    Clear,
    /// The rest of the line after the command's name, trimmed.
    Select(&'a str),
    Find(&'a str),
    Open(&'a str),
    Play,
    Stop,
    Save,
    Check,
    /// Anything else.
    Lua(&'a str),
}

/// Tells built-ins from Lua. A built-in's name must be the first word, with
/// nothing after it that makes it Lua instead: `clear` is the command,
/// `clear = 5` and `select(2, ...)` are Lua.
pub fn parse(input: &str) -> Command<'_> {
    let line = input.trim();
    let (word, rest) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(word, rest)| (word, rest.trim()));
    let lua = rest.starts_with(['=', '(', '.', ':', '[', '{', '"', '\''])
        || line.contains('\n')
        || word.contains(|c: char| !c.is_ascii_alphabetic());
    if lua {
        return Command::Lua(line);
    }
    match (word, rest.is_empty()) {
        ("help", true) => Command::Help,
        ("clear", true) => Command::Clear,
        ("play", true) => Command::Play,
        ("stop", true) => Command::Stop,
        ("save", true) => Command::Save,
        ("check", true) => Command::Check,
        ("select", _) => Command::Select(rest),
        ("find", _) => Command::Find(rest),
        ("open", _) => Command::Open(rest),
        _ => Command::Lua(line),
    }
}

/// Lua's reserved words, which start Lua, never a command.
const LUA_KEYWORDS: [&str; 22] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// The word a line probably meant as a command, such as `teleport` in
/// `teleport Baseplate`: one plain word that isn't Lua's, then nothing or
/// more words.
pub fn unknown_command(line: &str) -> Option<&str> {
    let line = line.trim();
    let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let plain = word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && word.starts_with(|c: char| c.is_ascii_alphabetic());
    let words_follow = rest
        .trim_start()
        .chars()
        .next()
        .is_none_or(|c| c.is_alphanumeric() || c == '_');
    let lua = LUA_KEYWORDS.contains(&word) || axcenity_script::global_names().contains(&word);
    (plain && words_follow && !lua && !line.contains('\n')).then_some(word)
}

/// Finds an Instance by path or name. A path is dotted (`Workspace.Door`,
/// `workspace.Door` or `game.Workspace.Door`); a single name is searched
/// for everywhere, first match in Explorer order.
pub fn find_instance(world: &World, text: &str) -> Option<InstanceId> {
    let text = text.trim();
    if text.contains('.') {
        return resolve_path(world, text);
    }
    if let Some(service) = resolve_path(world, text) {
        return Some(service);
    }
    world.find_first_descendant(world.root(), text)
}

/// Every Instance whose name contains `text`, ignoring case, in Explorer order.
pub fn find_all(world: &World, text: &str) -> Vec<InstanceId> {
    let wanted = text.trim().to_lowercase();
    world
        .descendants(world.root())
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.name().to_lowercase().contains(&wanted))
        })
        .collect()
}

/// The Instance at a dotted path. The first part is `game`, `workspace`, or
/// one of `game`'s children (a service, such as `Workspace` or `Lighting`).
fn resolve_path(world: &World, path: &str) -> Option<InstanceId> {
    let mut parts = path.split('.');
    let mut current = match parts.next()? {
        "game" => world.root(),
        "workspace" => world.workspace(),
        first => world.find_first_child(world.root(), first)?,
    };
    for part in parts {
        current = world.find_first_child(current, part)?;
    }
    Some(current)
}

/// What Tab could put in place of the word before the cursor.
#[derive(Debug, PartialEq, Eq)]
pub struct Completion {
    /// Where the part being completed starts, in bytes.
    pub start: usize,
    /// Every name that fits, sorted.
    pub candidates: Vec<String>,
}

/// The completions for the text before the cursor, `before`. After a dot,
/// they're the children and properties of the Instance the path names;
/// otherwise they're global names (and built-ins, as the first word).
pub fn complete(before: &str, world: &World, globals: &[&str]) -> Completion {
    let token_start = before
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .map_or(0, |at| {
            at + before[at..].chars().next().map_or(1, char::len_utf8)
        });
    let token = &before[token_start..];
    let first_word = before[..token_start].trim().is_empty();
    let command = parse(before);
    let takes_path = matches!(command, Command::Select(_) | Command::Find(_));

    let (start, prefix, mut candidates): (usize, &str, Vec<String>) = match token.rsplit_once('.') {
        Some((path, prefix)) => {
            let names = resolve_path(world, path)
                .map(|id| member_names(world, id))
                .unwrap_or_default();
            (token_start + path.len() + 1, prefix, names)
        }
        None => {
            let mut names: Vec<String> = Vec::new();
            if first_word {
                names.extend(BUILT_INS.iter().map(|(name, _)| name.to_string()));
            }
            if takes_path {
                names.extend(member_names(world, world.root()));
            } else {
                names.extend(globals.iter().map(|name| name.to_string()));
            }
            (token_start, token, names)
        }
    };
    let lower = prefix.to_lowercase();
    candidates.retain(|name| name.to_lowercase().starts_with(&lower) && name != prefix);
    candidates.sort();
    candidates.dedup();
    Completion { start, candidates }
}

/// The names that can follow `id` and a dot: its children with names Lua can
/// write that way, then its properties.
fn member_names(world: &World, id: InstanceId) -> Vec<String> {
    let Some(instance) = world.get(id) else {
        return Vec::new();
    };
    let identifier = |name: &str| {
        name.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
    };
    let children = instance
        .children()
        .iter()
        .filter_map(|&child| world.get(child))
        .map(|child| child.name().to_string())
        .filter(|name| identifier(name));
    let properties = instance.properties().map(|(name, _)| name.to_string());
    children
        .chain(properties)
        .chain(["Name", "Parent", "ClassName"].map(String::from))
        .collect()
}

/// The longest start every candidate shares, ignoring case. It takes the
/// first candidate's spelling.
fn common_prefix(candidates: &[String]) -> &str {
    let Some(first) = candidates.first() else {
        return "";
    };
    let mut end = first.len();
    for other in &candidates[1..] {
        let shared = first
            .char_indices()
            .zip(other.chars())
            .take_while(|((_, a), b)| a.eq_ignore_ascii_case(b))
            .last()
            .map_or(0, |((at, a), _)| at + a.len_utf8());
        end = end.min(shared);
    }
    &first[..end]
}

/// The command line: what's typed, the history, and the last completions.
#[derive(Default)]
pub struct Console {
    input: String,
    /// Commands run before, oldest first.
    history: Vec<String>,
    /// The history entry Up and Down are showing, or `None` while typing a
    /// new line.
    browsing: Option<usize>,
    /// The new line being typed before Up was pressed, which Down returns to.
    draft: String,
    /// What the last Tab could complete to, shown over the input when
    /// there's more than one.
    candidates: Vec<String>,
}

impl Console {
    /// The history as text, for eframe's storage.
    pub fn history_text(&self) -> String {
        serde_json::to_string(&self.history).unwrap_or_default()
    }

    /// Loads a history saved by [`history_text`](Self::history_text). A
    /// damaged one is left out.
    pub fn load_history(&mut self, text: &str) {
        self.history = serde_json::from_str(text).unwrap_or_default();
    }

    /// Adds a command to the history, unless it's the same as the last one.
    fn remember(&mut self, command: &str) {
        if self.history.last().map(String::as_str) != Some(command) {
            self.history.push(command.to_owned());
        }
        let extra = self.history.len().saturating_sub(HISTORY_LIMIT);
        self.history.drain(..extra);
        self.browsing = None;
        self.draft.clear();
    }

    /// Up (`back`) or Down through the history.
    fn walk(&mut self, back: bool) -> bool {
        let next = match (self.browsing, back) {
            (None, true) => self.history.len().checked_sub(1),
            (Some(at), true) => Some(at.saturating_sub(1)),
            (Some(at), false) if at + 1 < self.history.len() => Some(at + 1),
            (Some(_), false) => None,
            (None, false) => return false,
        };
        if next.is_none() && self.browsing.is_none() {
            return false;
        }
        if self.browsing.is_none() {
            self.draft = std::mem::take(&mut self.input);
        }
        self.browsing = next;
        self.input = match next {
            Some(at) => self.history[at].clone(),
            None => std::mem::take(&mut self.draft),
        };
        true
    }

    /// Completes the word before byte `cursor`, and returns where the
    /// cursor goes after it.
    fn complete_at(&mut self, cursor: usize, world: &World) -> usize {
        let globals = axcenity_script::global_names();
        let completion = complete(&self.input[..cursor], world, &globals);
        let typed = &self.input[completion.start..cursor];
        let replacement = match completion.candidates.as_slice() {
            [only] => only.as_str(),
            many => common_prefix(many),
        };
        self.candidates = if completion.candidates.len() > 1 {
            completion.candidates.clone()
        } else {
            Vec::new()
        };
        if replacement.len() < typed.len() {
            return cursor;
        }
        self.input
            .replace_range(completion.start..cursor, replacement);
        completion.start + replacement.len()
    }

    /// Draws the command line. Returns a command when Enter is pressed.
    /// `world` is for Tab completion.
    pub fn show(&mut self, ui: &mut egui::Ui, world: &World) -> Option<String> {
        let palette = theme::palette(ui.ctx());
        let id = ui.id().with("console input");
        let focused = ui.memory(|memory| memory.has_focus(id));
        let mut submitted = None;
        let mut move_cursor = None;
        if focused {
            let one_line = !self.input.contains('\n');
            // Taken before the text field sees them: Enter runs (Shift+Enter
            // is a new line), Tab completes, Up and Down walk the history.
            // `consume_key` ignores Shift, so it's checked first.
            let (enter, tab, up, down) = ui.input_mut(|input| {
                let plain = !input.modifiers.shift;
                let mut take = |key| plain && input.consume_key(egui::Modifiers::NONE, key);
                (
                    take(egui::Key::Enter),
                    take(egui::Key::Tab),
                    one_line && take(egui::Key::ArrowUp),
                    one_line && take(egui::Key::ArrowDown),
                )
            });
            if enter && !self.input.trim().is_empty() {
                let command = std::mem::take(&mut self.input);
                self.remember(command.trim_end());
                self.candidates.clear();
                submitted = Some(command.trim_end().to_owned());
            }
            if tab {
                let cursor = egui::TextEdit::load_state(ui.ctx(), id)
                    .and_then(|state| state.cursor.char_range())
                    .map_or(self.input.len(), |range| {
                        byte_index(&self.input, range.primary.index)
                    });
                move_cursor = Some(self.complete_at(cursor, world));
            }
            if (up && self.walk(true)) || (down && self.walk(false)) {
                self.candidates.clear();
                move_cursor = Some(self.input.len());
            }
        }

        if !self.candidates.is_empty() {
            let shown: Vec<&str> = self
                .candidates
                .iter()
                .take(30)
                .map(String::as_str)
                .collect();
            let more = self.candidates.len().saturating_sub(shown.len());
            let mut text = shown.join("  ");
            if more > 0 {
                text.push_str(&format!("  … {more} more"));
            }
            ui.add(
                egui::Label::new(
                    egui::RichText::new(text)
                        .font(egui::TextStyle::Monospace.resolve(ui.style()))
                        .color(palette.text_muted),
                )
                .wrap(),
            );
        }

        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let response = ui
            .horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    egui::RichText::new(icons::CARET_RIGHT)
                        .font(egui::FontId::proportional(font.size))
                        .color(palette.accent_text),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.input)
                        .id(id)
                        .font(font.clone())
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        // Shift+Enter makes a new line; Enter is taken above.
                        .return_key(egui::KeyboardShortcut::new(
                            egui::Modifiers::SHIFT,
                            egui::Key::Enter,
                        ))
                        // Tab stays in the field, for completion.
                        .lock_focus(true)
                        .hint_text(
                            egui::RichText::new("Lua, or a command such as help")
                                .color(palette.text_muted),
                        ),
                )
            })
            .inner;
        if response.changed() {
            self.candidates.clear();
            self.browsing = None;
        }
        if let Some(at) = move_cursor
            && let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id)
        {
            let index = self.input[..at].chars().count();
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(index))));
            state.store(ui.ctx(), id);
        }
        if submitted.is_some() {
            response.request_focus();
        }
        submitted
    }
}

/// The byte index of the `chars`th character of `text`.
fn byte_index(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(at, _)| at)
}

#[cfg(test)]
mod tests {
    use axcenity_core::ClassName;

    use super::*;

    fn world() -> (World, InstanceId) {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, "Baseplate").unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        (world, part)
    }

    #[test]
    fn built_ins_are_told_from_lua() {
        assert_eq!(parse("help"), Command::Help);
        assert_eq!(parse("  clear "), Command::Clear);
        assert_eq!(
            parse("select Workspace.Baseplate"),
            Command::Select("Workspace.Baseplate")
        );
        assert_eq!(parse("open Spinner"), Command::Open("Spinner"));
        assert_eq!(parse("find"), Command::Find(""));
        assert_eq!(parse("check"), Command::Check);
        for lua in [
            "clear = 5",
            "select(2, 'a', 'b')",
            "play()",
            "print(workspace.Baseplate.Size)",
            "help\nprint(1)",
            "save me",
        ] {
            assert_eq!(parse(lua), Command::Lua(lua.trim()), "{lua}");
        }
    }

    #[test]
    fn unknown_commands_are_one_plain_word_then_more() {
        assert_eq!(unknown_command("teleport Baseplate"), Some("teleport"));
        assert_eq!(unknown_command("teleport"), Some("teleport"));
        for lua in ["x = = 1", "(1 +", "local x y", "print hello", "a\nb c"] {
            assert_eq!(unknown_command(lua), None, "{lua}");
        }
    }

    #[test]
    fn instances_are_found_by_path_or_name() {
        let (world, part) = world();
        assert_eq!(find_instance(&world, "Baseplate"), Some(part));
        assert_eq!(find_instance(&world, "Workspace.Baseplate"), Some(part));
        assert_eq!(find_instance(&world, "workspace.Baseplate"), Some(part));
        assert_eq!(
            find_instance(&world, "game.Workspace.Baseplate"),
            Some(part)
        );
        assert_eq!(find_instance(&world, "Workspace"), Some(world.workspace()));
        assert_eq!(find_instance(&world, "Nope"), None);
        assert_eq!(find_all(&world, "base"), vec![part]);
    }

    #[test]
    fn tab_completes_paths_properties_globals_and_built_ins() {
        let (world, _) = world();
        let globals = ["print", "pairs", "workspace", "game"];
        let names = |before: &str| complete(before, &world, &globals).candidates;
        assert_eq!(names("workspace.Bas"), ["Baseplate"]);
        assert!(names("workspace.Baseplate.Transp").contains(&"Transparency".to_string()));
        assert_eq!(names("print(wor"), ["workspace"]);
        assert_eq!(names("pr"), ["print"]);
        assert_eq!(names("he"), ["help"]);
        assert_eq!(names("select Work"), ["Workspace"]);
        assert_eq!(complete("x = workspace.Bas", &world, &globals).start, 14);

        let mut console = Console {
            input: "workspace.bas".into(),
            ..Console::default()
        };
        let cursor = console.complete_at(13, &world);
        assert_eq!(console.input, "workspace.Baseplate");
        assert_eq!(cursor, console.input.len());
    }

    #[test]
    fn up_and_down_walk_the_history_and_come_back_to_the_draft() {
        let mut console = Console::default();
        console.remember("print(1)");
        console.remember("print(2)");
        console.remember("print(2)");
        console.input = "pri".into();
        assert!(console.walk(true));
        assert_eq!(console.input, "print(2)");
        assert!(console.walk(true));
        assert_eq!(console.input, "print(1)");
        assert!(console.walk(true));
        assert_eq!(console.input, "print(1)", "stays at the oldest");
        assert!(console.walk(false));
        assert!(console.walk(false));
        assert_eq!(console.input, "pri");
        assert!(!console.walk(false));

        let mut reloaded = Console::default();
        reloaded.load_history(&console.history_text());
        assert_eq!(reloaded.history, ["print(1)", "print(2)"]);
    }
}
