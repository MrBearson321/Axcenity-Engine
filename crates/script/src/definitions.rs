//! Writes the luau-lsp definitions file: `declare extern type` and `declare`
//! statements describing the whole script API, with `---` doc comments, so
//! editors such as VS Code can autocomplete it and type-check scripts as Luau.
//!
//! - Classes, properties, events and enums come straight from `core`, so they
//!   can't fall out of date. A new property only needs a line in
//!   `PROPERTY_DOCS`, and a new event one in `EVENT_DOCS`.
//! - APIs that only work on one side (see `sides`) say so in their
//!   description, added from `SIDE_ONLY_APIS`.
//! - Functions, globals and value types are described by hand below.
//! - Luau's own globals (`math`, `bit32`, `buffer`, `typeof`, ...) are built
//!   into luau-lsp, so they aren't declared here.
//! - Tests check the descriptions against what scripts really get, and that
//!   the checked-in file matches this generator.
//!
//! luau-lsp calls the format unstable and undocumented; it is Luau's own
//! definition syntax, as in luau-lsp's Roblox definitions.

use axcenity_core::{ClassName, ValueKind, World};

use crate::sides::{Side, side_only};

mod docs;
mod tables;

use crate::types::EnumType;
use docs::*;
use tables::*;

/// Where the generated file is kept, relative to the repository root.
pub const DEFINITIONS_PATH: &str = "definitions/axcenity.d.luau";

/// The command that rewrites the file, shown in its header and in test failures.
const REGENERATE: &str = "cargo run -p axcenity-script --bin lua-definitions";

/// Children found by name, such as `workspace.Pad`. A child could be of any
/// class, and Luau only lets you use a property every member of a union has,
/// so a union of every class would flag `workspace.Pad.Color = ...`; `any`
/// lets it through unchecked. Every Instance class needs its own: Luau
/// doesn't inherit indexers.
const CHILD_INDEXER: &str = "\t[string]: any";

/// The class of `Enum.<name>`'s items, such as `EnumMaterial`. Luau class
/// names can't have dots, so this is the type for `Enum.Material.Neon`.
fn enum_item_class(enum_type: EnumType) -> String {
    format!("Enum{}", enum_type.name())
}

/// The Luau type for a property of this kind.
fn luau_type(kind: ValueKind) -> String {
    match kind {
        ValueKind::Bool => "boolean".to_owned(),
        ValueKind::Number => "number".to_owned(),
        ValueKind::String => "string".to_owned(),
        ValueKind::Vector3 => "Vector3".to_owned(),
        ValueKind::UDim2 => "UDim2".to_owned(),
        ValueKind::Color3 => "Color3".to_owned(),
        // Enum properties read as items and also accept item names.
        ValueKind::Material
        | ValueKind::PartType
        | ValueKind::TextureChannel
        | ValueKind::ParticleType => {
            let enum_type = EnumType::of_kind(kind).expect("an enum kind");
            let mut options = vec![enum_item_class(enum_type)];
            options.extend(
                enum_type
                    .item_names()
                    .iter()
                    .map(|name| format!("\"{name}\"")),
            );
            options.join(" | ")
        }
    }
}

/// `name: type` for each parameter. `("x?", "number")` becomes `x: number?`,
/// and `("...", "any")` becomes `...: any` in a declared function, or
/// `...any` in a function type (`in_type`), as Luau writes them.
fn parameters(function: &Function, in_type: bool) -> Vec<String> {
    function
        .params
        .iter()
        .map(|&(name, ty)| match name.strip_suffix('?') {
            // A union or function type needs brackets before `?`.
            Some(name) if ty.contains(['|', '-']) => format!("{name}: ({ty})?"),
            Some(name) => format!("{name}: {ty}?"),
            None if name == "..." && in_type => format!("...{ty}"),
            None => format!("{name}: {ty}"),
        })
        .collect()
}

/// Collects the file's text one line at a time.
struct Writer(String);

impl Writer {
    fn line(&mut self, text: &str) {
        self.0.push_str(text);
        self.0.push('\n');
    }

    /// A doc comment, which luau-lsp shows on hover and in autocomplete.
    fn doc(&mut self, indent: &str, text: &str) {
        if !text.is_empty() {
            self.line(&format!("{indent}--- {text}"));
        }
    }

    /// A class member: `Name: type`.
    fn member(&mut self, name: &str, ty: &str, doc: &str) {
        self.doc("\t", doc);
        self.line(&format!("\t{name}: {ty}"));
    }

    fn field(&mut self, field: &Field) {
        self.member(field.name, field.ty, &with_side(field.name, field.doc));
    }

    /// A method, called with `:`.
    fn method(&mut self, method: &Function) {
        self.doc("\t", &with_side(method.name, method.doc));
        let mut params = vec!["self".to_owned()];
        params.extend(parameters(method, false));
        self.line(&format!(
            "\tfunction {}({}): {}",
            method.name,
            params.join(", "),
            method.returns
        ));
    }

    fn heading(&mut self, title: &str) {
        // Sections already end with a blank line.
        if !self.0.ends_with("\n\n") {
            self.line("");
        }
        self.line(&format!("-- {title} {}", "-".repeat(74 - title.len())));
        self.line("");
    }

    /// Ends a `declare extern type` and leaves a blank line after it.
    fn end(&mut self) {
        self.line("end");
        self.line("");
    }
}

/// The text of the definitions file for the current API.
pub fn lua_definitions() -> String {
    let mut w = Writer(String::new());
    w.line("-- The Axcenity Engine script API, for luau-lsp (autocomplete and type");
    w.line("-- checking in VS Code and other editors).");
    w.line(&format!(
        "-- Generated by `{REGENERATE}`. Don't edit by hand."
    ));

    w.heading("Values");
    for value_type in VALUE_TYPES {
        w.doc("", value_type.doc);
        w.line(&format!("declare extern type {} with", value_type.class));
        for field in value_type.fields {
            w.field(field);
        }
        for operator in value_type.operators {
            w.line(&format!("\tfunction {operator}"));
        }
        for method in value_type.methods {
            w.method(method);
        }
        w.end();
    }

    w.heading("Enums");
    for enum_type in EnumType::ALL {
        let name = enum_type.name();
        let item = enum_item_class(enum_type);
        w.doc("", &format!("One choice of `Enum.{name}`."));
        w.line(&format!("declare extern type {item} extends EnumItem with"));
        w.end();
        w.doc("", &format!("The choices of `Enum.{name}`."));
        w.line(&format!("declare extern type {item}Items with"));
        for choice in enum_type.item_names() {
            w.line(&format!("\t{choice}: {item}"));
        }
        w.end();
    }
    w.doc("", ENUM_GLOBAL_DOC);
    w.line("declare extern type EnumLibrary with");
    for enum_type in EnumType::ALL {
        let name = enum_type.name();
        w.line(&format!("\t{name}: {}Items", enum_item_class(enum_type)));
    }
    w.end();

    w.heading("Instances");
    w.doc("", SIGNAL_DOC);
    w.line("declare extern type Signal with");
    for method in SIGNAL_METHODS {
        w.method(method);
    }
    w.end();
    w.doc("", CONNECTION_DOC);
    w.line("declare extern type Connection with");
    for field in CONNECTION_FIELDS {
        w.field(field);
    }
    for method in CONNECTION_METHODS {
        w.method(method);
    }
    w.end();

    w.doc(
        "",
        "Anything in the game tree. Every class below is a kind of Instance.",
    );
    w.line("declare extern type Instance with");
    for field in INSTANCE_FIELDS {
        w.field(field);
    }
    for (event, doc) in INSTANCE_EVENT_DOCS {
        w.member(event, "Signal", doc);
    }
    w.line(CHILD_INDEXER);
    for method in INSTANCE_METHODS {
        w.method(method);
    }
    w.end();

    // The services, as `game` holds them, so `game.Workspace` autocompletes.
    let world = World::new();
    let services: Vec<ClassName> = world
        .get(world.root())
        .map(|game| {
            game.children()
                .iter()
                .filter_map(|&id| world.get(id).map(|s| s.class()))
                .collect()
        })
        .unwrap_or_default();

    for class in ClassName::ALL {
        w.doc("", class_doc(class));
        w.line(&format!(
            "declare extern type {class} extends Instance with"
        ));
        if class == ClassName::DataModel {
            for &service in &services {
                w.member(service.as_str(), service.as_str(), class_doc(service));
            }
        }
        for (name, default) in class.default_properties() {
            let doc = property_doc(class, name).unwrap_or("");
            w.member(name, &luau_type(default.kind()), doc);
        }
        for (_, field) in CLASS_FIELDS.iter().filter(|(c, _)| *c == class) {
            w.field(field);
        }
        for &event in class.events() {
            let doc = event_doc(class, event).unwrap_or("");
            w.member(event, "Signal", &with_side(event, doc));
        }
        w.line(CHILD_INDEXER);
        for (_, method) in CLASS_METHODS.iter().filter(|(c, _)| *c == class) {
            w.method(method);
        }
        w.end();
    }

    w.heading("Globals");
    for &(name, ty, doc) in INSTANCE_GLOBALS {
        w.doc("", doc);
        w.line(&format!("declare {name}: {ty}"));
        w.line("");
    }
    w.doc("", ENUM_GLOBAL_DOC);
    w.line("declare Enum: EnumLibrary");
    w.line("");
    for function in GLOBAL_FUNCTIONS {
        w.doc("", &with_side(function.name, function.doc));
        w.line(&format!(
            "declare function {}({}): {}",
            function.name,
            parameters(function, false).join(", "),
            function.returns
        ));
        w.line("");
    }

    // Each library is a class of function fields (called with `.`), so its
    // functions can have doc comments, and one global holds it.
    for library in LIBRARIES {
        let class = format!("{}Library", library.global);
        w.doc("", library.doc);
        w.line(&format!("declare extern type {class} with"));
        for function in library.functions {
            let ty = function_type(library, function);
            w.member(function.name, &ty, &with_side(function.name, function.doc));
        }
        w.end();
        w.doc("", library.doc);
        w.line(&format!("declare {}: {class}", library.global));
        w.line("");
    }

    // One blank line at the end, not two.
    let mut text = w.0;
    while text.ends_with("\n\n") {
        text.pop();
    }
    text
}

/// A library function's type, such as `(x: number?) -> Vector3`.
/// `Instance.new("Part")` returns a `Part`, so `Instance.new` is the
/// intersection of one function per class (Luau's way to write overloads),
/// then the plain one for any other name.
fn function_type(library: &Library, function: &Function) -> String {
    let plain = format!(
        "({}) -> {}",
        parameters(function, true).join(", "),
        function.returns
    );
    if library.global != "Instance" || function.name != "new" {
        return plain;
    }
    let mut overloads: Vec<String> = ClassName::ALL
        .into_iter()
        // Services always exist, and Players are made when someone joins.
        .filter(|class| !class.is_service() && *class != ClassName::Player)
        .map(|class| format!("((className: \"{class}\") -> {class})"))
        .collect();
    overloads.push(format!("({plain})"));
    overloads.join(" & ")
}

/// `doc`, saying where it works if `name` is a side-only API.
fn with_side(name: &str, doc: &str) -> String {
    match side_only(name).map(|api| api.side) {
        Some(Side::Client) => format!("{doc} LocalScripts only."),
        Some(Side::Server) => format!("{doc} Scripts only."),
        None => doc.to_owned(),
    }
}

fn event_doc(class: ClassName, event: &str) -> Option<&'static str> {
    EVENT_DOCS
        .iter()
        .find(|(c, e, _)| *c == class && *e == event)
        .map(|(_, _, doc)| *doc)
}

fn property_doc(class: ClassName, property: &str) -> Option<&'static str> {
    PROPERTY_DOCS
        .iter()
        .find(|(c, p, _)| *c == class && *p == property)
        .map(|(_, _, doc)| *doc)
}

#[cfg(test)]
mod tests;
