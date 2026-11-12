//! Completion in code tabs (roadmap G4, stage 1): what can follow the
//! cursor, and the signature of the call it's in.
//!
//! It reads the API from `axcenity_script::api_reference()`, the same entries
//! the definitions file for luau-lsp and the API Reference page come from,
//! and the world being edited for Instance paths. Types are followed through
//! a chain such as `game:GetService("Players").LocalPlayer.Character`, and
//! through a local's declaration (`local part = workspace.Door`, `local p:
//! Part`, `Instance.new("Part")`). No type checking: anything it can't
//! follow simply offers nothing.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use axcenity_core::{InstanceId, World};
use axcenity_script::{ApiEntry, ApiKind};

use crate::lua_syntax::{self, TokenKind};

/// How far back a call's opening bracket is looked for.
const SIGNATURE_REACH: usize = 2_000;
/// The most items a list offers.
const MAX_ITEMS: usize = 200;
/// How many local declarations deep a type is followed.
const MAX_DEPTH: usize = 6;

const KEYWORDS: [&str; 22] = [
    "and", "break", "continue", "do", "else", "elseif", "end", "false", "for", "function", "if",
    "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// What an item is, for its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Keyword,
    /// A local variable or function in the script.
    Local,
    Global,
    /// A child of an Instance in the world.
    Child,
    Property,
    Event,
    Function,
    Enum,
    EnumItem,
    /// A name written as text: a service, class, property, attribute,
    /// child or tag inside a call's string argument.
    Name,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub kind: ItemKind,
    /// How it's written or called, such as `Part.Size: Vector3`.
    pub detail: String,
    pub doc: String,
}

/// Where Go to Definition (F12 in a code tab) takes the name at the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Definition {
    /// Declared in this script (a local, a function, a parameter or a loop
    /// variable): the byte where its name is written there.
    Here(usize),
    /// An Instance in the world being edited (`workspace.Door`, a
    /// `require`d ModuleScript): opened as a double-click on its Explorer row
    /// opens it, so a script shows in its tab.
    Instance(InstanceId),
    /// A member or global of the engine's API, by its API Reference name
    /// (`BasePart.Touched`, `task.wait`).
    Api(String),
}

/// What can replace the word before the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completions {
    /// Where the word being completed starts, in bytes.
    pub start: usize,
    pub items: Vec<Item>,
}

/// The call the cursor is inside: its signature and which argument it's on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub label: String,
    pub doc: String,
    /// Counting from 0.
    pub argument: usize,
}

/// The completions at byte `cursor` of `source`, if any fit. `world` gives
/// Instance paths their children.
pub fn complete(source: &str, cursor: usize, world: &World) -> Option<Completions> {
    let cursor = floor_char(source, cursor);
    if in_string_or_comment(source, cursor) {
        return arguments::complete(source, cursor, world);
    }
    let before = &source[..cursor];
    let start = before.rfind(|c: char| !is_word(c)).map_or(0, |at| {
        at + before[at..].chars().next().map_or(1, char::len_utf8)
    });
    let prefix = &before[start..];
    if prefix.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let api = api();
    let mut items = match before[..start].chars().next_back() {
        Some(access @ ('.' | ':')) => {
            let receiver = receiver_start(before, start - 1);
            // `..` joins strings; it isn't a member.
            if access == '.' && before[..start - 1].ends_with('.') {
                return None;
            }
            let ty = Resolver::new(source, cursor, world).chain(&before[receiver..start - 1])?;
            members(api, world, &ty, access == ':')
        }
        _ => {
            if prefix.is_empty() {
                return None;
            }
            names_in_scope(api, before)
        }
    };
    let lower = prefix.to_lowercase();
    items.retain(|item| item.label.to_lowercase().starts_with(&lower));
    // Exact-case matches first, then by name.
    items.sort_by(|a, b| {
        (
            !a.label.starts_with(prefix),
            a.label.to_lowercase(),
            &a.label,
        )
            .cmp(&(
                !b.label.starts_with(prefix),
                b.label.to_lowercase(),
                &b.label,
            ))
    });
    items.dedup_by(|a, b| a.label == b.label);
    items.truncate(MAX_ITEMS);
    // Nothing to offer once the word is typed out in full.
    if items.is_empty() || (items.len() == 1 && items[0].label == prefix) {
        return None;
    }
    Some(Completions { start, items })
}

/// The signature of the engine call the cursor is inside, such as
/// `Instance:FindFirstChild(name: string, recursive: boolean?): Instance?`.
pub fn signature_at(source: &str, cursor: usize, world: &World) -> Option<Signature> {
    let cursor = floor_char(source, cursor);
    if in_string_or_comment(source, cursor) {
        return None;
    }
    let reach = floor_char(source, cursor.saturating_sub(SIGNATURE_REACH));
    let window = &source[reach..cursor];
    // The innermost bracket still open, and the commas at its level.
    let mut depth = 0usize;
    let mut commas = 0;
    let mut open = None;
    for (at, c) in window.char_indices().rev() {
        match c {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth > 0 => depth -= 1,
            '(' => {
                open = Some(reach + at);
                break;
            }
            '[' | '{' => return None,
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    let open = open?;
    let callee = &source[..open];
    let name_start = callee.rfind(|c: char| !is_word(c)).map_or(0, |at| at + 1);
    let name = &callee[name_start..];
    if name.is_empty() {
        return None;
    }
    let api = api();
    let entry = match callee[..name_start].chars().next_back() {
        Some(access @ ('.' | ':')) => {
            let receiver = receiver_start(callee, name_start - 1);
            let ty =
                Resolver::new(source, cursor, world).chain(&callee[receiver..name_start - 1])?;
            member(api, &ty, name, access == ':')?
        }
        _ => api
            .globals
            .iter()
            .find(|entry| entry.name == name && entry.kind == ApiKind::Function)?,
    };
    Some(Signature {
        label: entry.signature.clone(),
        doc: entry.doc.clone(),
        argument: commas,
    })
}

/// The API, sorted for lookups once.
struct Api {
    /// Members of each class or type, reached on a value.
    members: HashMap<String, Vec<ApiEntry>>,
    /// Members reached on a global table (`Vector3.new`, `task.wait`).
    libraries: HashMap<String, Vec<ApiEntry>>,
    /// Globals, global functions and libraries.
    globals: Vec<ApiEntry>,
    /// The Instance classes.
    classes: HashSet<String>,
    /// `Material` for `Enum.Material`, with each one's entry.
    enums: Vec<ApiEntry>,
    /// Each enum's items, by `Enum.Material`.
    enum_items: HashMap<String, Vec<ApiEntry>>,
    /// Lua's own globals and the sandbox's, such as `print` and `math`.
    builtins: Vec<&'static str>,
    /// The services that aren't in the game tree (`RunService`, `Debris`),
    /// read from their descriptions: each says it comes from
    /// `game:GetService("Name")`.
    service_names: Vec<String>,
}

fn api() -> &'static Api {
    static API: OnceLock<Api> = OnceLock::new();
    API.get_or_init(|| {
        let mut api = Api {
            members: HashMap::new(),
            libraries: HashMap::new(),
            globals: Vec::new(),
            classes: HashSet::new(),
            enums: Vec::new(),
            enum_items: HashMap::new(),
            builtins: axcenity_script::global_names(),
            service_names: Vec::new(),
        };
        for entry in axcenity_script::api_reference() {
            if entry.kind == ApiKind::Type
                && entry
                    .doc
                    .contains(&format!("GetService(\"{}\")", entry.name))
            {
                api.service_names.push(entry.name.clone());
            }
            match entry.kind {
                ApiKind::Class => {
                    api.classes.insert(entry.name.clone());
                }
                ApiKind::Enum => api.enums.push(entry),
                ApiKind::EnumItem => api
                    .enum_items
                    .entry(entry.owner.clone())
                    .or_default()
                    .push(entry),
                ApiKind::Global | ApiKind::Library => api.globals.push(entry),
                _ if entry.owner.is_empty() => api.globals.push(entry),
                _ if entry.library => api
                    .libraries
                    .entry(entry.owner.clone())
                    .or_default()
                    .push(entry),
                _ => api
                    .members
                    .entry(entry.owner.clone())
                    .or_default()
                    .push(entry),
            }
        }
        api
    })
}

/// Where the name at byte `cursor` of `source` is defined (roadmap G4):
/// its declaration in the script, the Instance a chain names, or the API
/// entry of a member or global. `None` for a keyword, a string, a comment,
/// or a name nothing here can follow.
pub fn definition_at(source: &str, cursor: usize, world: &World) -> Option<Definition> {
    let cursor = floor_char(source, cursor);
    let start = source[..cursor]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(cursor, |(at, _)| at);
    let end = source[cursor..]
        .char_indices()
        .find(|(_, c)| !is_word(*c))
        .map_or(source.len(), |(at, _)| cursor + at);
    let word = &source[start..end];
    if !is_identifier(word) || KEYWORDS.contains(&word) || in_string_or_comment(source, start) {
        return None;
    }
    let api = api();
    let receiver = receiver_start(source, end);
    if receiver == start {
        let code = without_strings_and_comments(source);
        if let Some(at) = declared_at(&code[..end], word).or_else(|| global_function(&code, word)) {
            return Some(Definition::Here(at));
        }
        return match word {
            "game" => Some(Definition::Instance(world.root())),
            "workspace" => Some(Definition::Instance(world.workspace())),
            _ if api.libraries.contains_key(word)
                || api.globals.iter().any(|entry| entry.name == word) =>
            {
                Some(Definition::Api(word.to_owned()))
            }
            _ => None,
        };
    }
    let mut resolver = Resolver::new(source, start, world);
    if let Some(Ty::Instance { id: Some(id), .. }) = resolver.chain(&source[receiver..end]) {
        return Some(Definition::Instance(id));
    }
    // A member: the API entry on what's before the `.` or `:`.
    let colon = source[..start].ends_with(':');
    let owner = resolver.chain(&source[receiver..start - 1])?;
    let entry = member(api, &owner, word, colon).or_else(|| member(api, &owner, word, !colon))?;
    Some(Definition::Api(entry.qualified_name()))
}

/// Where the last declaration of `name` in `code` (strings and comments
/// blanked) writes it: `local a, name`, `local function name`, `function
/// name(`, a parameter `function (x, name)`, or `for _, name in`.
fn declared_at(code: &str, name: &str) -> Option<usize> {
    let mut line_end = code.len();
    loop {
        let line_start = code[..line_end].rfind('\n').map_or(0, |at| at + 1);
        let line = &code[line_start..line_end];
        let found = whole_words(line, name)
            .filter(|&at| declares(&line[..at]))
            .last();
        if let Some(at) = found {
            return Some(line_start + at);
        }
        if line_start == 0 {
            return None;
        }
        line_end = line_start - 1;
    }
}

/// Whether a name written after `prefix` (the start of its line) is being
/// declared there.
fn declares(prefix: &str) -> bool {
    let names = |text: &str| {
        text.chars()
            .all(|c| is_word(c) || matches!(c, ' ' | '\t' | ',' | ':' | '?' | '|' | '<' | '>'))
    };
    let trimmed = prefix.trim_start();
    if let Some(rest) = trimmed.strip_prefix("local ") {
        let rest = rest.trim_start();
        if rest.is_empty() || rest == "function " || names(rest) {
            return true;
        }
    }
    if trimmed == "function " {
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix("for ") {
        return names(rest) && !rest.contains(':');
    }
    // A parameter: `function name(a, ` or `function(`.
    if let Some(open) = prefix.rfind('(')
        && names(&prefix[open + 1..])
    {
        let head = prefix[..open].trim_end();
        let head = head.trim_end_matches(|c: char| is_word(c) || c == '.' || c == ':');
        return head.trim_end().ends_with("function");
    }
    false
}

/// A global `function name(` anywhere in `code`, declared after its use.
fn global_function(code: &str, name: &str) -> Option<usize> {
    let mut offset = 0;
    for line in code.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("function ")
            && rest.trim_start().starts_with(name)
            && rest.trim_start()[name.len()..]
                .trim_start()
                .starts_with('(')
        {
            let at = line.len() - rest.trim_start().len();
            return Some(offset + at);
        }
        offset += line.len();
    }
    None
}

/// The byte where each whole-word `word` starts in `text`.
fn whole_words<'a>(text: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(word).filter_map(move |(at, _)| {
        let end = at + word.len();
        let before_ok = !text[..at].chars().next_back().is_some_and(is_word);
        let after_ok = !text[end..].chars().next().is_some_and(is_word);
        (before_ok && after_ok).then_some(at)
    })
}

/// What an expression is, as far as completion can tell.
#[derive(Clone, Debug, PartialEq)]
enum Ty {
    /// An Instance of `class`, and which one when the world has it.
    Instance {
        class: String,
        id: Option<InstanceId>,
    },
    /// A value of a type that isn't an Instance, such as `Vector3`.
    Value(String),
    /// A global table, such as `task`, or `Vector3` for `Vector3.new`.
    Library(String),
    /// `Enum`.
    Enums,
    /// `Enum.Material`.
    Enum(String),
    /// A function not called yet: its owner (empty for a global) and entry
    /// name, for `Instance.new(...)` and the like.
    Function { owner: String, name: String },
}

/// The type a type annotation or an entry's `value` names, if completion
/// can follow it: `Part?` is a Part, `Instance | nil` an Instance.
fn parse_type(api: &Api, text: &str) -> Option<Ty> {
    let first = text.split('|').next()?.trim().trim_end_matches('?').trim();
    if first.is_empty() || first.starts_with(['{', '(']) {
        return None;
    }
    if first == "Instance" || api.classes.contains(first) {
        return Some(Ty::Instance {
            class: first.to_owned(),
            id: None,
        });
    }
    if first == "EnumLibrary" {
        return Some(Ty::Enums);
    }
    api.members
        .contains_key(first)
        .then(|| Ty::Value(first.to_owned()))
}

/// Follows expressions to their types.
struct Resolver<'a> {
    source: &'a str,
    /// Locals are looked for before here.
    cursor: usize,
    world: &'a World,
    depth: usize,
}

impl<'a> Resolver<'a> {
    fn new(source: &'a str, cursor: usize, world: &'a World) -> Self {
        Self {
            source,
            cursor,
            world,
            depth: 0,
        }
    }

    /// The type of a chain such as `game:GetService("Players").LocalPlayer`.
    fn chain(&mut self, text: &str) -> Option<Ty> {
        let api = api();
        let mut segments = segments(text.trim())?.into_iter();
        let Segment::Name(first) = segments.next()? else {
            return None;
        };
        let mut ty = self.name(api, &first)?;
        for segment in segments {
            ty = match segment {
                Segment::Name(_) => return None,
                Segment::Dot(name) | Segment::Index(name) => self.dot(api, &ty, &name)?,
                Segment::Method(name, argument) => self.method(api, &ty, &name, argument)?,
                Segment::Call(argument) => self.call(api, &ty, argument)?,
            };
        }
        Some(ty)
    }

    /// A name on its own: a local, else a global.
    fn name(&mut self, api: &Api, name: &str) -> Option<Ty> {
        if let Some(ty) = self.local(api, name) {
            return Some(ty);
        }
        let world = self.world;
        match name {
            "game" => Some(Ty::Instance {
                class: "DataModel".to_owned(),
                id: Some(world.root()),
            }),
            "workspace" => Some(Ty::Instance {
                class: "Workspace".to_owned(),
                id: Some(world.workspace()),
            }),
            "script" => Some(Ty::Instance {
                class: "Script".to_owned(),
                id: None,
            }),
            "Enum" => Some(Ty::Enums),
            _ if api.libraries.contains_key(name) => Some(Ty::Library(name.to_owned())),
            _ => {
                let global = api.globals.iter().find(|entry| entry.name == name)?;
                match global.kind {
                    ApiKind::Function => Some(Ty::Function {
                        owner: String::new(),
                        name: name.to_owned(),
                    }),
                    _ => parse_type(api, &global.value),
                }
            }
        }
    }

    /// `ty.name`, or `ty["name"]`.
    fn dot(&mut self, api: &Api, ty: &Ty, name: &str) -> Option<Ty> {
        match ty {
            Ty::Instance { class, id } => {
                // A child by that name comes first, as in Lua.
                if let Some(child) = id.and_then(|id| self.world.find_first_child(id, name)) {
                    let class = self.world.get(child)?.class().as_str().to_owned();
                    return Some(Ty::Instance {
                        class,
                        id: Some(child),
                    });
                }
                let entry = member(api, ty, name, false)?;
                if entry.kind == ApiKind::Function {
                    return None;
                }
                let mut found = parse_type(api, &entry.value)?;
                // `game.Players` names the service the world has.
                if class == "DataModel"
                    && let Ty::Instance { id: found_id, .. } = &mut found
                {
                    *found_id = id.and_then(|id| self.world.find_first_child(id, name));
                }
                Some(found)
            }
            Ty::Value(_) => parse_type(api, &member(api, ty, name, false)?.value),
            Ty::Library(owner) => {
                let entry = member(api, ty, name, false)?;
                if entry.kind == ApiKind::Function {
                    Some(Ty::Function {
                        owner: owner.clone(),
                        name: name.to_owned(),
                    })
                } else {
                    parse_type(api, &entry.value)
                }
            }
            Ty::Enums => {
                let qualified = format!("Enum.{name}");
                api.enum_items
                    .contains_key(&qualified)
                    .then_some(Ty::Enum(qualified))
            }
            Ty::Enum(_) => Some(Ty::Value("EnumItem".to_owned())),
            Ty::Function { .. } => None,
        }
    }

    /// `ty:name(argument, ...)`.
    fn method(&mut self, api: &Api, ty: &Ty, name: &str, argument: Option<String>) -> Option<Ty> {
        if let Ty::Instance { class, id } = ty {
            let world = self.world;
            match (name, argument) {
                ("GetService", Some(service)) => {
                    let id = world.find_first_child(world.root(), &service);
                    let class = id
                        .and_then(|id| world.get(id))
                        .map_or(service, |instance| instance.class().as_str().to_owned());
                    return Some(Ty::Instance { class, id });
                }
                ("FindFirstChild" | "WaitForChild", Some(child)) => {
                    if let Some(child) = id.and_then(|id| world.find_first_child(id, &child)) {
                        let class = world.get(child)?.class().as_str().to_owned();
                        return Some(Ty::Instance {
                            class,
                            id: Some(child),
                        });
                    }
                }
                (
                    "FindFirstChildOfClass" | "FindFirstChildWhichIsA" | "FindFirstAncestorOfClass",
                    Some(class),
                ) if api.classes.contains(&class) => {
                    return Some(Ty::Instance { class, id: None });
                }
                ("Clone", _) => {
                    return Some(Ty::Instance {
                        class: class.clone(),
                        id: None,
                    });
                }
                _ => {}
            }
        }
        let entry = member(api, ty, name, true)?;
        parse_type(api, &entry.value)
    }

    /// Calling a function value: `Instance.new("Part")`, `Vector3.new()`.
    fn call(&mut self, api: &Api, ty: &Ty, argument: Option<String>) -> Option<Ty> {
        let Ty::Function { owner, name } = ty else {
            return None;
        };
        if owner == "Instance" && name == "new" {
            let class = argument.filter(|class| api.classes.contains(class))?;
            return Some(Ty::Instance { class, id: None });
        }
        let entries = if owner.is_empty() {
            &api.globals
        } else {
            api.libraries.get(owner)?
        };
        let entry = entries.iter().find(|entry| &entry.name == name)?;
        parse_type(api, &entry.value)
    }

    /// A local declared before the cursor: its annotation, else what it's
    /// set to.
    fn local(&mut self, api: &Api, name: &str) -> Option<Ty> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        let (annotation, value) = declaration(&self.source[..self.cursor], name)?;
        if let Some(annotation) = annotation {
            return parse_type(api, &annotation);
        }
        let value = value?;
        self.depth += 1;
        // The declaration's own value is looked up before it.
        let at = self.source[..self.cursor]
            .rfind(&value)
            .unwrap_or(self.cursor);
        let outer = std::mem::replace(&mut self.cursor, at);
        let ty = self.chain(&value);
        self.cursor = outer;
        self.depth -= 1;
        ty
    }
}

/// One step of a chain.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    Name(String),
    Dot(String),
    /// `["Name"]`.
    Index(String),
    /// `:Name(...)`, with its first argument when that's a string.
    Method(String, Option<String>),
    /// `(...)`, with its first argument when that's a string.
    Call(Option<String>),
}

/// The steps of a chain, or `None` where it has something else in it.
fn segments(text: &str) -> Option<Vec<Segment>> {
    let mut out = Vec::new();
    let mut rest = text;
    let word = |text: &str| -> (String, usize) {
        let end = text.find(|c: char| !is_word(c)).unwrap_or(text.len());
        (text[..end].to_owned(), end)
    };
    while !rest.is_empty() {
        rest = rest.trim_start();
        if out.is_empty() {
            let (name, end) = word(rest);
            if name.is_empty() {
                return None;
            }
            out.push(Segment::Name(name));
            rest = &rest[end..];
            continue;
        }
        let mut chars = rest.chars();
        match chars.next()? {
            '.' => {
                let (name, end) = word(&rest[1..]);
                if name.is_empty() {
                    return None;
                }
                out.push(Segment::Dot(name));
                rest = &rest[1 + end..];
            }
            ':' => {
                let (name, end) = word(&rest[1..]);
                let after = rest[1 + end..].trim_start();
                if name.is_empty() || !after.starts_with('(') {
                    return None;
                }
                let (inside, len) = bracketed(after)?;
                out.push(Segment::Method(name, string_argument(inside)));
                rest = &after[len..];
            }
            '(' => {
                let (inside, len) = bracketed(rest)?;
                out.push(Segment::Call(string_argument(inside)));
                rest = &rest[len..];
            }
            '[' => {
                let (inside, len) = bracketed(rest)?;
                out.push(Segment::Index(string_argument(inside)?));
                rest = &rest[len..];
            }
            _ => return None,
        }
    }
    Some(out)
}

/// What's inside the bracket `text` starts with, and the length to just
/// past its closing one.
fn bracketed(text: &str) -> Option<(&str, usize)> {
    let mut depth = 0;
    let mut quote = None;
    for (at, c) in text.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(' | '[' | '{') => depth += 1,
            (None, ')' | ']' | '}') => {
                depth -= 1;
                if depth == 0 {
                    return Some((&text[1..at], at + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// The first argument, when it's a plain string: `"Players"` from
/// `"Players", true`.
fn string_argument(inside: &str) -> Option<String> {
    let first = inside.split(',').next()?.trim();
    let quote = first.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    first
        .strip_prefix(quote)?
        .strip_suffix(quote)
        .filter(|text| !text.contains(quote))
        .map(str::to_owned)
}

/// Where the chain ending at byte `end` of `text` starts: back over names,
/// `.` and `:`, and bracketed calls and indexes.
fn receiver_start(text: &str, end: usize) -> usize {
    let bytes = text.as_bytes();
    let mut at = end;
    loop {
        // A name.
        while at > 0 && is_word(bytes[at - 1] as char) {
            at -= 1;
        }
        // Spaces may come before `(` in a call, never elsewhere here.
        let Some(&before) = at.checked_sub(1).and_then(|i| bytes.get(i)) else {
            return at;
        };
        match before {
            b'.' | b':' if at > 1 && bytes[at - 2] != b'.' => at -= 1,
            b')' | b']' => {
                let (open, close) = if before == b')' {
                    (b'(', b')')
                } else {
                    (b'[', b']')
                };
                let mut depth = 0;
                let mut i = at;
                let mut found = None;
                while i > 0 {
                    i -= 1;
                    if bytes[i] == close {
                        depth += 1;
                    } else if bytes[i] == open {
                        depth -= 1;
                        if depth == 0 {
                            found = Some(i);
                            break;
                        }
                    }
                }
                match found {
                    Some(open_at) => at = open_at,
                    None => return at,
                }
            }
            _ => return at,
        }
    }
}

/// The last `local name[: Type] [= value]` (or `name: Type` parameter, or
/// `for _, name in` loop over children) in `before`: its annotation and its
/// value's text up to the end of the line.
fn declaration(before: &str, name: &str) -> Option<(Option<String>, Option<String>)> {
    for line in before.lines().rev() {
        let code = line.split("--").next().unwrap_or("");
        // `local a, name = ...` and `local name: Type = ...`.
        if let Some(locals) = code.trim_start().strip_prefix("local ") {
            let (names, value) = match locals.split_once('=') {
                Some((names, value)) => (names, Some(value.trim())),
                None => (locals, None),
            };
            let names = if locals.trim_start().starts_with("function ") {
                ""
            } else {
                names
            };
            for (index, declared) in names.split(',').enumerate() {
                let (declared, annotation) = match declared.split_once(':') {
                    Some((declared, ty)) => (declared.trim(), Some(ty.trim().to_owned())),
                    None => (declared.trim(), None),
                };
                if declared == name {
                    // Only the first name takes a single value's type.
                    let value = value.filter(|_| index == 0).map(str::to_owned);
                    return Some((annotation, value));
                }
            }
        }
        // A typed parameter: `function (part: Part, ...)`.
        if code.contains("function")
            && let Some(params) = code
                .split_once('(')
                .and_then(|(_, rest)| rest.split(')').next())
        {
            for param in params.split(',') {
                if let Some((declared, ty)) = param.split_once(':')
                    && declared.trim() == name
                {
                    return Some((Some(ty.trim().to_owned()), None));
                }
            }
        }
        // `for _, child in workspace:GetChildren() do`: an Instance.
        if let Some(header) = code.trim_start().strip_prefix("for ")
            && let Some((names, _)) = header.split_once(" in ")
            && names.split(',').any(|declared| declared.trim() == name)
            && (header.contains("GetChildren()") || header.contains("GetDescendants()"))
        {
            return Some((Some("Instance".to_owned()), None));
        }
    }
    None
}

/// The entry for `name` on `ty`: a method when `colon`, else anything else.
/// Instance classes also have Instance's own members.
fn member<'a>(api: &'a Api, ty: &Ty, name: &str, colon: bool) -> Option<&'a ApiEntry> {
    let fits = |entry: &&ApiEntry| entry.name == name && (entry.kind == ApiKind::Function) == colon;
    match ty {
        Ty::Instance { class, .. } => api
            .members
            .get(class)
            .into_iter()
            .chain(api.members.get("Instance"))
            .flatten()
            .find(fits),
        Ty::Value(owner) => api.members.get(owner)?.iter().find(fits),
        Ty::Library(owner) => api
            .libraries
            .get(owner)?
            .iter()
            .find(|entry| entry.name == name && !colon),
        _ => None,
    }
}

/// Everything that can follow `ty` and `.` (or `:`, `colon`).
fn members(api: &Api, world: &World, ty: &Ty, colon: bool) -> Vec<Item> {
    let item = |entry: &ApiEntry| Item {
        label: entry.name.clone(),
        kind: match entry.kind {
            ApiKind::Event => ItemKind::Event,
            ApiKind::Function => ItemKind::Function,
            ApiKind::EnumItem => ItemKind::EnumItem,
            _ => ItemKind::Property,
        },
        detail: entry.signature.clone(),
        doc: entry.doc.clone(),
    };
    let fits = |entry: &&ApiEntry| (entry.kind == ApiKind::Function) == colon;
    match ty {
        Ty::Instance { class, id } => {
            let mut items: Vec<Item> = Vec::new();
            if !colon && let Some(instance) = id.and_then(|id| world.get(id)) {
                for &child in instance.children() {
                    let Some(child) = world.get(child) else {
                        continue;
                    };
                    if is_identifier(child.name()) {
                        items.push(Item {
                            label: child.name().to_owned(),
                            kind: ItemKind::Child,
                            detail: format!("{}  ({})", child.name(), child.class()),
                            doc: String::new(),
                        });
                    }
                }
            }
            items.extend(
                api.members
                    .get(class.as_str())
                    .into_iter()
                    .chain(api.members.get("Instance"))
                    .flatten()
                    .filter(fits)
                    .map(item),
            );
            items
        }
        Ty::Value(owner) => api
            .members
            .get(owner)
            .into_iter()
            .flatten()
            .filter(fits)
            .map(item)
            .collect(),
        Ty::Library(owner) if !colon => api
            .libraries
            .get(owner)
            .into_iter()
            .flatten()
            .map(item)
            .collect(),
        Ty::Enums if !colon => api
            .enums
            .iter()
            .map(|entry| Item {
                label: entry.name.trim_start_matches("Enum.").to_owned(),
                kind: ItemKind::Enum,
                detail: entry.signature.clone(),
                doc: entry.doc.clone(),
            })
            .collect(),
        Ty::Enum(qualified) if !colon => api
            .enum_items
            .get(qualified)
            .into_iter()
            .flatten()
            .map(item)
            .collect(),
        _ => Vec::new(),
    }
}

/// Keywords, the script's own locals and functions, and globals.
fn names_in_scope(api: &Api, before: &str) -> Vec<Item> {
    let mut items: Vec<Item> = KEYWORDS
        .iter()
        .map(|keyword| Item {
            label: (*keyword).to_owned(),
            kind: ItemKind::Keyword,
            detail: String::new(),
            doc: String::new(),
        })
        .collect();
    for name in local_names(before) {
        items.push(Item {
            label: name,
            kind: ItemKind::Local,
            detail: "local".to_owned(),
            doc: String::new(),
        });
    }
    for entry in &api.globals {
        items.push(Item {
            label: entry.name.clone(),
            kind: if entry.kind == ApiKind::Function {
                ItemKind::Function
            } else {
                ItemKind::Global
            },
            detail: entry.signature.clone(),
            doc: entry.doc.clone(),
        });
    }
    for name in &api.builtins {
        items.push(Item {
            label: (*name).to_owned(),
            kind: ItemKind::Global,
            detail: String::new(),
            doc: String::new(),
        });
    }
    items
}

/// The names declared with `local` (variables and functions), as
/// parameters, or as loop variables, before the cursor.
fn local_names(before: &str) -> Vec<String> {
    let code = without_strings_and_comments(before);
    let mut names = Vec::new();
    let mut add = |name: &str| {
        let name = name.split(':').next().unwrap_or("").trim();
        if is_identifier(name) && !KEYWORDS.contains(&name) {
            names.push(name.to_owned());
        }
    };
    // `local a, b = ...`, `local function f`, `for k, v in` and `for i = `.
    for keyword in ["local", "for"] {
        for after in after_word(&code, keyword) {
            let list = after.split(['=', '(', ')', '\n']).next().unwrap_or("");
            let list = list.split(" in ").next().unwrap_or("");
            let list = list.trim_start().strip_prefix("function ").unwrap_or(list);
            list.split(',').for_each(&mut add);
        }
    }
    // Function names and parameters: `function name(a, b: Part)`.
    for after in after_word(&code, "function") {
        if let Some((name, rest)) = after.split_once('(') {
            add(name);
            if let Some(params) = rest.split(')').next() {
                params.split(',').for_each(&mut add);
            }
        }
    }
    names
}

/// The text after each whole-word `word` in `code`.
fn after_word<'a>(code: &'a str, word: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '_';
    code.match_indices(word).filter_map(move |(at, _)| {
        let end = at + word.len();
        let before_ok = !code[..at].chars().next_back().is_some_and(is_name);
        let after_ok = !code[end..].chars().next().is_some_and(is_name);
        (before_ok && after_ok).then(|| &code[end..])
    })
}

/// `source` with its strings and comments blanked out (newlines kept), so
/// words inside them aren't read as code.
fn without_strings_and_comments(source: &str) -> String {
    let mut code = source.to_owned();
    for (kind, range) in lua_syntax::tokens(source) {
        if matches!(kind, TokenKind::String | TokenKind::Comment) {
            // One space per byte, so byte offsets still line up.
            let blank: String = source[range.clone()]
                .chars()
                .map(|c| match c {
                    '\n' => "\n".to_owned(),
                    c => " ".repeat(c.len_utf8()),
                })
                .collect();
            code.replace_range(range, &blank);
        }
    }
    code
}

/// Whether byte `cursor` is inside a string or a comment: one that runs up
/// to it and isn't closed before it.
fn in_string_or_comment(source: &str, cursor: usize) -> bool {
    let before = &source[..cursor];
    let Some((kind, range)) = lua_syntax::tokens(before).pop() else {
        return false;
    };
    if range.end != cursor {
        return false;
    }
    let text = &before[range];
    match kind {
        // A line comment runs to the end of its line.
        TokenKind::Comment => !text.starts_with("--[") || !text.ends_with("]]") || text.len() < 6,
        TokenKind::String => {
            let closed = match text.chars().next() {
                Some(quote @ ('"' | '\'' | '`')) => {
                    text.len() >= 2
                        && text.ends_with(quote)
                        && !text.ends_with(&format!("\\{quote}"))
                }
                Some('[') => text.len() >= 4 && text.ends_with("]]"),
                _ => true,
            };
            !closed
        }
        _ => false,
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_identifier(name: &str) -> bool {
    name.starts_with(|c: char| c.is_alphabetic() || c == '_') && name.chars().all(is_word)
}

/// `at`, or the start of the character it's inside.
fn floor_char(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

mod arguments;
pub(crate) mod lint;

#[cfg(test)]
mod tests;
