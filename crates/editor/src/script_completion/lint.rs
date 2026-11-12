//! The editor's own checks of a code tab, which need no luau-lsp: syntax
//! errors (from the Luau compiler, as the script would be compiled), and
//! names the API says can't be there, followed through the same types as
//! completion: `part:Destory()`, `Enum.Materal`, `Enum.Material.Neone`,
//! `game:GetService("Playres")`, `Instance.new("Prat")`,
//! `part:GetPropertyChangedSignal("Colour")` and `workspace.Dor` when the
//! Workspace being edited has no `Dor`.
//!
//! A name completion can't follow (a table, a function's result, a class
//! with no Instance behind it, where a child may be added while the game
//! runs) is never reported, so nothing here is a guess: it can miss
//! problems, but shouldn't report one that isn't.

use std::collections::HashMap;

use axcenity_core::World;
use axcenity_script::ApiKind;

use super::{
    Api, Resolver, Ty, api, arguments, is_word, member, receiver_start,
    without_strings_and_comments,
};
use crate::luau_check::{Diagnostic, Severity};

/// Longer scripts get the syntax check only, so a huge file can't make
/// typing slow.
const MAX_CHECKED_BYTES: usize = 300_000;

/// What's wrong with `source`, as luau-lsp's findings are given.
pub fn check(source: &str, world: &World) -> Vec<Diagnostic> {
    let lines = LineStarts::new(source);
    if let Some(error) = axcenity_script::find_syntax_error(source) {
        let line = error.line.max(1);
        let length = source.lines().nth(line - 1).map_or(0, str::len);
        return vec![Diagnostic {
            line,
            column: 1,
            end_line: line,
            end_column: length.max(1),
            severity: Severity::Error,
            code: "SyntaxError".to_owned(),
            message: error.message,
        }];
    }
    if source.len() > MAX_CHECKED_BYTES {
        return Vec::new();
    }
    let api = api();
    let code = without_strings_and_comments(source);
    let mut found = Vec::new();
    let mut types: HashMap<&str, Option<Ty>> = HashMap::new();
    let bytes = code.as_bytes();
    for (at, c) in code.char_indices() {
        if c != '.' && c != ':' {
            continue;
        }
        // `..` joins strings, `...` is the varargs and `::` a type cast.
        let before = at.checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(at + 1).copied();
        if matches!(before, Some(b'.' | b':')) || matches!(after, Some(b'.' | b':')) {
            continue;
        }
        // A number such as `1.5`.
        if before.is_some_and(|b| b.is_ascii_digit())
            && code[..at]
                .rsplit(|c: char| !is_word(c))
                .next()
                .is_some_and(|word| word.starts_with(|c: char| c.is_ascii_digit()))
        {
            continue;
        }
        let name_end = code[at + 1..]
            .find(|c: char| !is_word(c))
            .map_or(code.len(), |end| at + 1 + end);
        let name = &code[at + 1..name_end];
        if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let receiver_from = receiver_start(&code, at);
        let receiver = &source[receiver_from..at];
        if receiver.trim().is_empty() || receiver.contains('\n') {
            continue;
        }
        // `function Module.name()` and `function Class:name()` declare it.
        if code[..receiver_from].trim_end().ends_with("function") {
            continue;
        }
        let ty = types
            .entry(receiver)
            .or_insert_with(|| Resolver::new(source, at, world).chain(receiver))
            .clone();
        let Some(ty) = ty else {
            continue;
        };
        if let Some((severity, message)) = unknown(api, world, &ty, name, c == ':') {
            found.push(lines.diagnostic(at + 1, name_end, severity, "UnknownMember", message));
        }
    }
    string_names(source, &code, world, api, &lines, &mut found);
    found.sort_by_key(|found| (found.line, found.column));
    found
}

/// Why `ty` can't have `name` (after `:` when `colon`), if it can't.
fn unknown(
    api: &Api,
    world: &World,
    ty: &Ty,
    name: &str,
    colon: bool,
) -> Option<(Severity, String)> {
    match ty {
        Ty::Instance { class, id } => {
            if member(api, ty, name, colon).is_some() || member(api, ty, name, !colon).is_some() {
                return None;
            }
            if colon {
                return Some((
                    Severity::Error,
                    format!("{name} is not a method of {class}"),
                ));
            }
            // Only an Instance in the world can be known to have no such
            // child, and only until a script adds one.
            let id = (*id)?;
            let instance = world.get(id)?;
            if world.find_first_child(id, name).is_some() {
                return None;
            }
            let place = world
                .full_name(id)
                .filter(|place| !place.is_empty())
                .unwrap_or_else(|| instance.name().to_owned());
            Some((
                Severity::Warning,
                format!(
                    "{name} is not a property or event of {class}, and {place} has no child \
                     named {name} in the editor (if a script adds it, use {place}:WaitForChild(\"{name}\"))"
                ),
            ))
        }
        Ty::Value(owner) => {
            if member(api, ty, name, colon).is_some() || member(api, ty, name, !colon).is_some() {
                return None;
            }
            Some((
                Severity::Warning,
                if colon {
                    format!("{name} is not a method of {owner}")
                } else {
                    format!("{name} is not a property of {owner}")
                },
            ))
        }
        Ty::Enums if !colon => {
            let qualified = format!("Enum.{name}");
            (!api.enums.iter().any(|entry| entry.name == qualified)).then(|| {
                (
                    Severity::Error,
                    format!("Enum.{name} doesn't exist; see the API Reference's Enums"),
                )
            })
        }
        Ty::Enum(qualified) if !colon => {
            let items = api.enum_items.get(qualified)?;
            (!items.iter().any(|item| item.name == name)).then(|| {
                let choices: Vec<&str> = items.iter().map(|item| item.name.as_str()).collect();
                (
                    Severity::Error,
                    format!(
                        "{qualified}.{name} doesn't exist; choose from {}",
                        choices.join(", ")
                    ),
                )
            })
        }
        _ => None,
    }
}

/// Names written as text that must be real: `GetService("Name")`,
/// `Instance.new("Class")` and `GetPropertyChangedSignal("Property")`.
fn string_names(
    source: &str,
    code: &str,
    world: &World,
    api: &Api,
    lines: &LineStarts,
    found: &mut Vec<Diagnostic>,
) {
    let services: Vec<String> = arguments::services(api, world)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for call in ["GetService(", "Instance.new(", "GetPropertyChangedSignal("] {
        let mut from = 0;
        while let Some(found_at) = code[from..].find(call) {
            let at = from + found_at;
            from = at + call.len();
            // A whole name, not `MyGetService(`.
            if code[..at].chars().next_back().is_some_and(is_word) {
                continue;
            }
            let Some((text, start, end)) = quoted(source, from) else {
                continue;
            };
            let problem = match call {
                "GetService(" => (!services.iter().any(|service| service == text))
                    .then(|| format!("there is no service named \"{text}\"")),
                "Instance.new(" => (!api.classes.contains(text))
                    .then(|| format!("\"{text}\" is not a class of Instance")),
                _ => {
                    // The receiver must be a known class to know its properties.
                    let receiver_end = at.checked_sub(1).filter(|&i| code.as_bytes()[i] == b':');
                    let Some(receiver_end) = receiver_end else {
                        continue;
                    };
                    let receiver = &source[receiver_start(code, receiver_end)..receiver_end];
                    let Some(ty) = Resolver::new(source, at, world).chain(receiver) else {
                        continue;
                    };
                    let Ty::Instance { class, .. } = &ty else {
                        continue;
                    };
                    member(api, &ty, text, false)
                        .is_none_or(|entry| entry.kind != ApiKind::Property)
                        .then(|| format!("{text} is not a property of {class}"))
                }
            };
            if let Some(message) = problem {
                found.push(lines.diagnostic(start, end, Severity::Error, "UnknownName", message));
            }
        }
    }
}

/// The plain string starting at byte `from` (after spaces): its text and
/// the bytes it covers inside the quotes.
fn quoted(source: &str, from: usize) -> Option<(&str, usize, usize)> {
    let rest = &source[from..];
    let skipped = rest.len() - rest.trim_start().len();
    let start = from + skipped;
    let quote = source[start..]
        .chars()
        .next()
        .filter(|c| matches!(c, '"' | '\''))?;
    let inside = start + 1;
    let length = source[inside..].find([quote, '\n', '\\'])?;
    (source[inside + length..].starts_with(quote))
        .then(|| (&source[inside..inside + length], inside, inside + length))
}

/// Where each line starts, to turn bytes into lines and columns.
struct LineStarts(Vec<usize>);

impl LineStarts {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(source.match_indices('\n').map(|(at, _)| at + 1));
        LineStarts(starts)
    }

    /// Line and column (from 1) of byte `at`.
    fn place(&self, at: usize) -> (usize, usize) {
        let line = self.0.partition_point(|&start| start <= at);
        (line, at - self.0[line - 1] + 1)
    }

    fn diagnostic(
        &self,
        start: usize,
        end: usize,
        severity: Severity,
        code: &str,
        message: String,
    ) -> Diagnostic {
        let (line, column) = self.place(start);
        let (end_line, end_column) = self.place(end.saturating_sub(1).max(start));
        Diagnostic {
            line,
            column,
            end_line,
            end_column,
            severity,
            code: code.to_owned(),
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::ClassName;

    use super::*;

    fn world() -> World {
        let mut world = World::new();
        let door = world.create(ClassName::Part).unwrap();
        world.set_name(door, "Door").unwrap();
        world.set_parent(door, Some(world.workspace())).unwrap();
        world
    }

    fn messages(source: &str) -> Vec<(usize, Severity, String)> {
        check(source, &world())
            .into_iter()
            .map(|found| (found.line, found.severity, found.message))
            .collect()
    }

    #[test]
    fn a_syntax_error_is_the_only_problem_reported() {
        let found = check("local x = workspace.Dor\nif x then\n", &world());
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, "SyntaxError");
        assert_eq!(found[0].line, 3);
    }

    #[test]
    fn correct_code_has_no_problems() {
        let source = r#"
local Players = game:GetService("Players")
local RunService = game:GetService("RunService")
local door = workspace.Door
door.Touched:Connect(function(other)
    other:Destroy()
end)
door.Color = Color3.new(1, 0, 0)
door.Material = Enum.Material.Neon
local v = Vector3.new(1, 2, 3).Unit
local text = "a.b" .. "c" .. tostring(1.5)
local copy = Instance.new("Part")
copy.Parent = workspace
door:GetPropertyChangedSignal("Transparency"):Connect(print)
local t = {}
t.anything = 1
function t.method() end
function t:other() end
print(door.Name, script.Parent, Players.LocalPlayer, ...)
"#;
        assert_eq!(messages(source), Vec::new());
    }

    #[test]
    fn misspelled_names_are_reported_where_they_are() {
        let source = "workspace.Door:Destory()\n\
                      local m = Enum.Materal.Neon\n\
                      local n = Enum.Material.Neone\n\
                      local p = game:GetService(\"Playres\")\n\
                      local q = Instance.new(\"Prat\")\n\
                      workspace.Door:GetPropertyChangedSignal(\"Colour\")\n\
                      print(workspace.Dor)\n";
        let found = check(source, &world());
        let summary: Vec<(usize, usize, Severity)> = found
            .iter()
            .map(|found| (found.line, found.column, found.severity))
            .collect();
        assert_eq!(
            summary,
            [
                (1, 16, Severity::Error),
                (2, 16, Severity::Error),
                (3, 25, Severity::Error),
                (4, 28, Severity::Error),
                (5, 25, Severity::Error),
                (6, 42, Severity::Error),
                (7, 17, Severity::Warning),
            ],
            "{found:#?}"
        );
        assert!(found[0].message.contains("Destory is not a method of Part"));
        assert!(
            found[2].message.contains("choose from"),
            "{}",
            found[2].message
        );
        assert!(
            found[6].message.contains("WaitForChild(\"Dor\")"),
            "{}",
            found[6].message
        );
    }

    #[test]
    fn names_that_may_be_added_while_the_game_runs_are_left_alone() {
        let source = "local part = Instance.new(\"Part\")\n\
                      print(part.Handle, script.Parent.Anything, game.Players.LocalPlayer.Character.Humanoid)\n";
        assert_eq!(messages(source), Vec::new());
    }
    #[test]
    fn a_long_script_is_checked_quickly() {
        let mut source = String::from("local values = {}\n");
        for i in 0..5_000 {
            source.push_str(&format!(
                "values[{i}] = workspace.Door.Position.X + {i}\nworkspace.Door:SetAttribute(\"n{i}\", values[{i}])\n"
            ));
        }
        let world = world();
        let started = std::time::Instant::now();
        let found = check(&source, &world);
        let elapsed = started.elapsed();
        eprintln!("10,000 lines checked in {elapsed:?}");
        assert_eq!(found.len(), 0, "{:?}", found.first());
        // A debug build; a release build is many times faster. It runs once
        // the tab has been still for 0.4 s, not on every key.
        assert!(elapsed.as_millis() < 1_000, "{elapsed:?}");
    }
}
